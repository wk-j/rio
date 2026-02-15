mod builtin;
mod runtime;

use crate::context::Context;
use librashader_common::{Size, Viewport};
use librashader_presets::ShaderFeatures;
use std::borrow::Cow;
use std::sync::Arc;

pub type Filter = String;

/// Resources for restoring the alpha channel after filter passes.
/// RetroArch shaders output alpha = 1.0, destroying window transparency.
/// This pipeline composites filtered RGB with the original pre-filter alpha.
struct AlphaRestore {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

impl AlphaRestore {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Alpha Restore Shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!(
                "shader/alpha_restore.wgsl"
            ))),
        });

        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Alpha Restore BGL"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float {
                                filterable: true,
                            },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float {
                                filterable: true,
                            },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(
                            wgpu::SamplerBindingType::Filtering,
                        ),
                        count: None,
                    },
                ],
            });

        let pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Alpha Restore Pipeline Layout"),
                bind_group_layouts: &[&bind_group_layout],
                immediate_size: 0,
            });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Alpha Restore Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Alpha Restore Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Self {
            pipeline,
            bind_group_layout,
            sampler,
        }
    }
}

/// A brush for applying RetroArch filters.
#[derive(Default)]
pub struct FiltersBrush {
    filter_chains: Vec<crate::components::filters::runtime::FilterChain>,
    filter_intermediates: Vec<Arc<wgpu::Texture>>,
    framecount: usize,
    alpha_restore: Option<AlphaRestore>,
}

impl FiltersBrush {
    #[inline]
    pub fn update_filters(&mut self, ctx: &Context, filters: &[Filter]) {
        self.filter_chains.clear();
        self.filter_intermediates.clear();

        if filters.is_empty() {
            self.alpha_restore = None;
            return;
        }

        let usage_caps = ctx.surface_caps().usages;

        if !usage_caps.contains(wgpu::TextureUsages::COPY_DST)
            || !usage_caps.contains(wgpu::TextureUsages::COPY_SRC)
        {
            tracing::warn!("The selected backend does not support the filter chains!");

            return;
        }

        for filter in filters {
            let configured_filter = filter.to_lowercase();
            match configured_filter.as_str() {
                "newpixiecrt" | "fubax_vr" => {
                    tracing::debug!("Loading builtin filter {}", configured_filter);

                    let builtin_filter = match configured_filter.as_str() {
                        "newpixiecrt" => builtin::newpixiecrt,
                        "fubax_vr" => builtin::fubaxvr,
                        _ => {
                            continue;
                        }
                    };

                    match builtin_filter() {
                        Ok(shader_preset) => {
                            match crate::components::filters::runtime::FilterChain::load_from_preset(
                                shader_preset,
                                &ctx.device,
                                &ctx.queue,
                                None,
                            ) {
                                Ok(f) => self.filter_chains.push(f),
                                Err(e) => tracing::error!("Failed to load builtin filter {}: {}", configured_filter, e),
                            }
                        },
                        Err(e) => {
                            tracing::error!("Failed to build shader preset from builtin filter {}: {}", configured_filter, e)
                        },
                    }
                }
                _ => {
                    tracing::debug!("Loading filter {}", filter);

                    match crate::components::filters::runtime::FilterChain::load_from_path(
                        filter,
                        ShaderFeatures::NONE,
                        &ctx.device,
                        &ctx.queue,
                        None,
                    ) {
                        Ok(f) => self.filter_chains.push(f),
                        Err(e) => {
                            tracing::error!("Failed to load filter {}: {}", filter, e)
                        }
                    }
                }
            }
        }

        self.filter_intermediates.reserve(self.filter_chains.len());

        // If we have an odd number of filters, the last filter can be
        // renderer directly to the output texture.
        let skip = if self.filter_chains.len() % 2 == 1 {
            1
        } else {
            0
        };

        let size = wgpu::Extent3d {
            depth_or_array_layers: 1,
            width: ctx.size.width as u32,
            height: ctx.size.height as u32,
        };

        for _ in self.filter_chains.iter().skip(skip) {
            let intermediate_texture =
                Arc::new(ctx.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("Filter Intermediate Texture"),
                    size,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: ctx.format,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING
                        | wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::COPY_SRC
                        | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[ctx.format],
                }));

            self.filter_intermediates.push(intermediate_texture);
        }

        // Initialize alpha restore pipeline for transparent background support
        if self.alpha_restore.is_none() && !self.filter_chains.is_empty() {
            self.alpha_restore = Some(AlphaRestore::new(&ctx.device, ctx.format));
        }
    }

    /// Render the filters on top of the src_texture to dst_texture.
    /// If the filters are not set, the src_texture is copied to dst_texture.
    /// After filter passes, the original alpha channel is restored to preserve
    /// window transparency (RetroArch shaders output alpha = 1.0).
    #[inline]
    pub fn render(
        &mut self,
        ctx: &Context,
        encoder: &mut wgpu::CommandEncoder,
        src_texture: &wgpu::Texture,
        dst_texture: &wgpu::Texture,
    ) {
        let filters_count = self.filter_chains.len();
        if filters_count == 0 {
            return;
        }

        let usage_caps = ctx.surface_caps().usages;

        if !usage_caps.contains(wgpu::TextureUsages::COPY_SRC)
            || !usage_caps.contains(wgpu::TextureUsages::COPY_DST)
        {
            return;
        }

        // Some shaders can do some specific things for which WGPU (at least the Vulkan backend)
        // requires the src and dst textures to be different, otherwise it will crash.
        // Also librashader requires a texture to be in Arc, so we need to make a copy anyway.
        // This copy also serves as the alpha source for the restore pass.
        let src_texture = {
            let new_src_texture =
                Arc::new(ctx.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("Filters Source Texture"),
                    size: src_texture.size(),
                    mip_level_count: src_texture.mip_level_count(),
                    sample_count: src_texture.sample_count(),
                    dimension: src_texture.dimension(),
                    format: src_texture.format(),
                    usage: wgpu::TextureUsages::TEXTURE_BINDING
                        | wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::COPY_SRC
                        | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[src_texture.format()],
                }));

            encoder.copy_texture_to_texture(
                src_texture.as_image_copy(),
                new_src_texture.as_image_copy(),
                new_src_texture.size(),
            );

            new_src_texture
        };

        // When alpha restore is active, the filter chain renders to an
        // intermediate texture instead of directly to dst_texture.
        // The alpha restore pass then composites filtered RGB + original alpha
        // into dst_texture.
        let has_alpha_restore = self.alpha_restore.is_some();
        let filter_output_texture: Option<Arc<wgpu::Texture>> = if has_alpha_restore {
            Some(Arc::new(ctx.device.create_texture(
                &wgpu::TextureDescriptor {
                    label: Some("Filter Output (pre-alpha-restore)"),
                    size: dst_texture.size(),
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: dst_texture.format(),
                    usage: wgpu::TextureUsages::TEXTURE_BINDING
                        | wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::COPY_SRC
                        | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[dst_texture.format()],
                },
            )))
        } else {
            None
        };

        // The effective destination for the filter chain: either the
        // intermediate texture (when alpha restore is active) or dst_texture.
        let effective_dst: &wgpu::Texture =
            filter_output_texture.as_deref().unwrap_or(dst_texture);

        let view_size = Size::new(ctx.size.width as u32, ctx.size.height as u32);
        for (idx, filter) in self.filter_chains.iter_mut().enumerate() {
            let filter_src_texture: Arc<wgpu::Texture>;
            let filter_dst_texture: &wgpu::Texture;

            if idx == 0 {
                filter_src_texture = src_texture.clone();

                if filters_count == 1 {
                    filter_dst_texture = effective_dst;
                } else {
                    filter_dst_texture = &self.filter_intermediates[0];
                }
            } else if idx == filters_count - 1 {
                filter_src_texture = self.filter_intermediates[idx - 1].clone();
                filter_dst_texture = effective_dst;
            } else {
                filter_src_texture = self.filter_intermediates[idx - 1].clone();
                filter_dst_texture = &self.filter_intermediates[idx];
            }

            let dst_texture_view =
                filter_dst_texture.create_view(&wgpu::TextureViewDescriptor::default());
            let dst_output_view =
                crate::components::filters::runtime::WgpuOutputView::new_from_raw(
                    &dst_texture_view,
                    view_size,
                    ctx.format,
                );
            let dst_viewport =
                Viewport::new_render_target_sized_origin(dst_output_view, None).unwrap();

            // Framecount should be added forever: https://github.com/raphamorim/rio/issues/753
            self.framecount = self.framecount.wrapping_add(1);
            if let Err(err) = filter.frame(
                filter_src_texture,
                &dst_viewport,
                encoder,
                self.framecount,
                None,
                ctx,
            ) {
                tracing::error!("Filter rendering failed: {err}");
            }
        }

        // Alpha restore pass: combine filtered RGB with original alpha
        // to preserve window transparency through the filter pipeline.
        if let (Some(alpha_restore), Some(filter_output)) =
            (&self.alpha_restore, &filter_output_texture)
        {
            let filtered_view =
                filter_output.create_view(&wgpu::TextureViewDescriptor::default());
            let original_view =
                src_texture.create_view(&wgpu::TextureViewDescriptor::default());

            let bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Alpha Restore BG"),
                layout: &alpha_restore.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&filtered_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&original_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&alpha_restore.sampler),
                    },
                ],
            });

            let dst_view =
                dst_texture.create_view(&wgpu::TextureViewDescriptor::default());
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Alpha Restore Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &dst_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            pass.set_pipeline(&alpha_restore.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}
