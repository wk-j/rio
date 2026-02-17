use rio_backend::config::colors::Colors;
use rio_backend::config::leader::LeaderItem;
use rio_backend::sugarloaf::{Object, Quad, RichText};

/// Draw the leader menu overlay
#[inline]
pub fn draw_leader_menu(
    objects: &mut Vec<Object>,
    rich_text_id: usize,
    colors: &Colors,
    items: &[LeaderItem],
    dimensions: (f32, f32, f32),
) {
    let (width, height, scale) = dimensions;
    let scaled_width = width / scale;
    let scaled_height = height / scale;

    // Menu dimensions - auto-size based on items
    let item_height = 20.0;
    let padding = 16.0;
    let menu_width = 220.0_f32.min(scaled_width - 20.0);
    let menu_height =
        (items.len() as f32 * item_height + padding * 4.0).min(scaled_height - 20.0);

    // Position at bottom-right with margin
    let margin = 10.0;
    let menu_x = scaled_width - menu_width - margin;
    let menu_y = scaled_height - menu_height - margin;

    // Menu background
    objects.push(Object::Quad(Quad {
        position: [menu_x, menu_y],
        color: colors.bar,
        size: [menu_width, menu_height],
        border_radius: [8.0, 8.0, 8.0, 8.0],
        ..Quad::default()
    }));

    // Border - use a slightly lighter version of the bar color
    let border_color = [
        (colors.bar[0] + 0.2).min(1.0),
        (colors.bar[1] + 0.2).min(1.0),
        (colors.bar[2] + 0.2).min(1.0),
        colors.bar[3],
    ];
    objects.push(Object::Quad(Quad {
        position: [menu_x - 1.0, menu_y - 1.0],
        color: border_color,
        size: [menu_width + 2.0, menu_height + 2.0],
        border_radius: [9.0, 9.0, 9.0, 9.0],
        border_width: 1.0,
        ..Quad::default()
    }));

    // Rich text for menu content
    objects.push(Object::RichText(RichText {
        id: rich_text_id,
        position: [menu_x + 16.0, menu_y + 8.0],
        lines: None,
    }));

    let _ = items; // Items will be rendered via the rich text
}

/// Format leader menu items for rich text display
pub fn format_leader_items(items: &[LeaderItem]) -> String {
    let mut output = String::new();
    output.push_str("Rio Commands\n\n");

    // Render items in 2 columns
    let half = (items.len() + 1) / 2;

    for (i, item) in items.iter().enumerate() {
        let key_display = match item.key {
            ' ' => "SPC".to_string(),
            '\n' => "RET".to_string(),
            '\t' => "TAB".to_string(),
            c => c.to_string(),
        };

        let line = format!("  {}  {}", key_display, item.label);
        output.push_str(&line);

        // Add newline or spacing for 2-column layout
        if i < half - 1 || i >= half {
            output.push('\n');
        } else {
            output.push_str("    ");
        }
    }

    output.push_str("\n\nPress key or Esc to cancel");
    output
}
