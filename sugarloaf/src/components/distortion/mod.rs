use crate::context::Context;
use bytemuck::{Pod, Zeroable};
use std::mem;

/// Distortion type constants matching the shader.
pub const DISTORTION_NONE: u32 = 0;
pub const DISTORTION_BARREL: u32 = 1;
pub const DISTORTION_PERSPECTIVE: u32 = 2;

/// GPU-side distortion parameters. Uploaded as a uniform buffer.
#[repr(C)]
#[derive(Debug, Clone, Copy, Zeroable, Pod)]
pub struct DistortionParams {
    /// 0=none, 1=barrel, 2=perspective
    pub distortion_type: u32,
    /// Effect magnitude (can be negative for inverse)
    pub strength: f32,
    /// Normalized center point [x, y]
    pub center: [f32; 2],
}

/// Post-processing brush that applies distortion effects to the
/// rendered frame via a full-screen triangle draw with distorted
/// UV sampling.
pub struct DistortionBrush {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    params_buffer: wgpu::Buffer,
    params_bind_group: wgpu::BindGroup,
    current_params: DistortionParams,
}

impl DistortionBrush {
    pub fn new(ctx: &Context) -> Self {
        let shader = ctx
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("sugarloaf::distortion shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("distortion.wgsl").into()),
            });

        // Bind group 0: source texture + sampler
        let bind_group_layout =
            ctx.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("sugarloaf::distortion texture layout"),
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
                            ty: wgpu::BindingType::Sampler(
                                wgpu::SamplerBindingType::Filtering,
                            ),
                            count: None,
                        },
                    ],
                });

        // Bind group 1: distortion params uniform
        let params_bind_group_layout =
            ctx.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("sugarloaf::distortion params layout"),
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    }],
                });

        let params = DistortionParams {
            distortion_type: DISTORTION_NONE,
            strength: 0.0,
            center: [0.5, 0.5],
        };

        let params_buffer = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sugarloaf::distortion params"),
            size: mem::size_of::<DistortionParams>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let params_bind_group =
            ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("sugarloaf::distortion params bind group"),
                layout: &params_bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params_buffer.as_entire_binding(),
                }],
            });

        let sampler = ctx.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sugarloaf::distortion sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let pipeline_layout =
            ctx.device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("sugarloaf::distortion pipeline layout"),
                    bind_group_layouts: &[&bind_group_layout, &params_bind_group_layout],
                    immediate_size: 0,
                });

        let pipeline =
            ctx.device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("sugarloaf::distortion pipeline"),
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vs_main"),
                        buffers: &[],
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some("fs_main"),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: ctx.format,
                            blend: Some(wgpu::BlendState::REPLACE),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
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

        Self {
            pipeline,
            bind_group_layout,
            sampler,
            params_buffer,
            params_bind_group,
            current_params: params,
        }
    }

    /// Update distortion parameters. Called when config changes.
    pub fn update_params(&mut self, queue: &wgpu::Queue, params: DistortionParams) {
        self.current_params = params;
        queue.write_buffer(&self.params_buffer, 0, bytemuck::bytes_of(&params));
    }

    /// Render the distortion pass. Copies src_texture, then
    /// draws a full-screen triangle with distorted UV sampling
    /// back to dst_texture.
    pub fn render(
        &self,
        ctx: &Context,
        encoder: &mut wgpu::CommandEncoder,
        src_texture: &wgpu::Texture,
        dst_texture: &wgpu::Texture,
    ) {
        if self.current_params.distortion_type == DISTORTION_NONE {
            return;
        }

        // Copy src to a temporary texture (can't read and
        // write the same texture in one pass)
        let src_copy = ctx.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("sugarloaf::distortion src copy"),
            size: src_texture.size(),
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: ctx.format,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        encoder.copy_texture_to_texture(
            src_texture.as_image_copy(),
            src_copy.as_image_copy(),
            src_texture.size(),
        );

        let src_view = src_copy.create_view(&wgpu::TextureViewDescriptor::default());
        let dst_view = dst_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let texture_bind_group =
            ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("sugarloaf::distortion tex bg"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&src_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("sugarloaf::distortion pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &dst_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &texture_bind_group, &[]);
        pass.set_bind_group(1, &self.params_bind_group, &[]);
        // Full-screen triangle: 3 vertices, 1 instance
        pass.draw(0..3, 0..1);
    }
}
