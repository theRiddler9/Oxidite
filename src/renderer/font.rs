//! Monospace font loading and cell metrics calculation.

use glyphon::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping};

/// Dimensions and metrics for a single monospace character cell.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellMetrics {
    /// Width of a single monospace cell in physical pixels.
    pub cell_width: f32,
    /// Height of a single monospace cell in physical pixels (line height).
    pub cell_height: f32,
    /// Configured font size in points/pixels.
    pub font_size: f32,
    /// Line height in points/pixels.
    pub line_height: f32,
}

impl CellMetrics {
    /// Measures cell dimensions for a monospace font family and size using `FontSystem`.
    pub fn measure(
        font_system: &mut FontSystem,
        font_size: f32,
        line_height_multiplier: f32,
    ) -> Self {
        let line_height = (font_size * line_height_multiplier).round().max(1.0);
        let metrics = Metrics::new(font_size, line_height);

        let mut buffer = Buffer::new(font_system, metrics);
        buffer.set_text(
            "M",
            &Attrs::new().family(Family::Monospace),
            Shaping::Advanced,
            None,
        );
        buffer.shape_until_scroll(font_system, false);

        // Find advance width of 'M' from layout runs
        let mut measured_width = font_size * 0.6; // fallback approximate monospace ratio
        for run in buffer.layout_runs() {
            for glyph in run.glyphs {
                if glyph.w > 0.0 {
                    measured_width = glyph.w;
                    break;
                }
            }
        }

        Self {
            cell_width: measured_width.ceil().max(1.0),
            cell_height: line_height,
            font_size,
            line_height,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measure_cell_metrics_produces_positive_dimensions() {
        let mut font_system = FontSystem::new();
        let metrics = CellMetrics::measure(&mut font_system, 14.0, 1.2);

        assert!(metrics.cell_width > 0.0);
        assert!(metrics.cell_height > 0.0);
        assert!((metrics.font_size - 14.0).abs() < f32::EPSILON);
        assert!(metrics.cell_height >= 14.0);
    }
}
