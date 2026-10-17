//! Cell backgrounds, glyphs, and cursor drawing.

use crate::grid::{Color, Cursor, CursorShape, Grid};
use crate::renderer::font::CellMetrics;
use glyphon::{
    Attrs as FontAttrs, Buffer as FontBuffer, Cache, Color as GlyphonColor, Family, FontSystem,
    Metrics, Resolution, Shaping, Style, SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer,
    Viewport, Weight,
};
use std::borrow::Cow;

/// Default 16-color standard ANSI palette (normalized RGBA).
pub const DEFAULT_PALETTE: [[f32; 4]; 16] = [
    [0.0, 0.0, 0.0, 1.0],    // 0: Black
    [0.8, 0.0, 0.0, 1.0],    // 1: Red
    [0.0, 0.8, 0.0, 1.0],    // 2: Green
    [0.8, 0.8, 0.0, 1.0],    // 3: Yellow
    [0.0, 0.0, 0.8, 1.0],    // 4: Blue
    [0.8, 0.0, 0.8, 1.0],    // 5: Magenta
    [0.0, 0.8, 0.8, 1.0],    // 6: Cyan
    [0.75, 0.75, 0.75, 1.0], // 7: White
    [0.5, 0.5, 0.5, 1.0],    // 8: Bright Black
    [1.0, 0.0, 0.0, 1.0],    // 9: Bright Red
    [0.0, 1.0, 0.0, 1.0],    // 10: Bright Green
    [1.0, 1.0, 0.0, 1.0],    // 11: Bright Yellow
    [0.0, 0.0, 1.0, 1.0],    // 12: Bright Blue
    [1.0, 0.0, 1.0, 1.0],    // 13: Bright Magenta
    [0.0, 1.0, 1.0, 1.0],    // 14: Bright Cyan
    [1.0, 1.0, 1.0, 1.0],    // 15: Bright White
];

/// Converts a terminal `Color` into a normalized `[r, g, b, a]` slice.
pub fn color_to_rgba(color: Color, default_color: [f32; 4], palette: &[[f32; 4]; 16]) -> [f32; 4] {
    match color {
        Color::Default => default_color,
        Color::Rgb(r, g, b) => [
            f32::from(r) / 255.0,
            f32::from(g) / 255.0,
            f32::from(b) / 255.0,
            1.0,
        ],
        Color::Indexed(idx) => {
            if (idx as usize) < 16 {
                palette[idx as usize]
            } else if (16..=231).contains(&idx) {
                let q = idx - 16;
                let r = (q / 36) % 6;
                let g = (q / 6) % 6;
                let b = q % 6;
                let conv = |c: u8| {
                    if c == 0 {
                        0.0
                    } else {
                        (f32::from(c) * 40.0 + 55.0) / 255.0
                    }
                };
                [conv(r), conv(g), conv(b), 1.0]
            } else {
                let gray = (f32::from(idx - 232) * 10.0 + 8.0) / 255.0;
                [gray, gray, gray, 1.0]
            }
        }
    }
}

/// Vertex format for drawing 2D colored quads (cell backgrounds and cursor).
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct QuadVertex {
    /// Position in physical pixels (x, y).
    pub position: [f32; 2],
    /// Normalized RGBA color.
    pub color: [f32; 4],
}

/// Converts a slice of `QuadVertex` to a byte vector.
fn vertices_to_bytes(vertices: &[QuadVertex]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(vertices.len() * 24);
    for v in vertices {
        bytes.extend_from_slice(&v.position[0].to_ne_bytes());
        bytes.extend_from_slice(&v.position[1].to_ne_bytes());
        bytes.extend_from_slice(&v.color[0].to_ne_bytes());
        bytes.extend_from_slice(&v.color[1].to_ne_bytes());
        bytes.extend_from_slice(&v.color[2].to_ne_bytes());
        bytes.extend_from_slice(&v.color[3].to_ne_bytes());
    }
    bytes
}

/// Builds 6 vertices (2 triangles) for an axis-aligned rectangle quad.
pub fn build_quad_vertices(x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) -> [QuadVertex; 6] {
    let p0 = [x, y];
    let p1 = [x, y + h];
    let p2 = [x + w, y];
    let p3 = [x + w, y + h];

    [
        QuadVertex {
            position: p0,
            color,
        },
        QuadVertex {
            position: p1,
            color,
        },
        QuadVertex {
            position: p2,
            color,
        },
        QuadVertex {
            position: p2,
            color,
        },
        QuadVertex {
            position: p1,
            color,
        },
        QuadVertex {
            position: p3,
            color,
        },
    ]
}

/// Gathers background quad vertices for all cells in `grid` that differ from default background.
#[allow(clippy::cast_precision_loss)]
pub fn build_cell_background_vertices(
    grid: &Grid,
    cell_width: f32,
    cell_height: f32,
    palette: &[[f32; 4]; 16],
) -> Vec<QuadVertex> {
    let mut vertices = Vec::new();

    for row in 0..grid.rows {
        let Some(row_cells) = grid.display_row(row) else {
            continue;
        };
        for col in 0..grid.cols {
            let cell = &row_cells[col];
            // Only draw background if cell background is not default
            if cell.bg != Color::Default {
                let x = col as f32 * cell_width;
                let y = row as f32 * cell_height;
                let w = f32::from(cell.width.max(1)) * cell_width;
                let h = cell_height;
                let color = color_to_rgba(cell.bg, [0.0, 0.0, 0.0, 1.0], palette);
                vertices.extend_from_slice(&build_quad_vertices(x, y, w, h, color));
            }
        }
    }

    vertices
}

/// Builds quad vertices for the cursor based on its position, visibility, and shape.
#[allow(clippy::cast_precision_loss)]
pub fn build_cursor_vertices(
    cursor: &Cursor,
    metrics: &CellMetrics,
    color: [f32; 4],
) -> Vec<QuadVertex> {
    if !cursor.visible {
        return Vec::new();
    }

    let x = cursor.col as f32 * metrics.cell_width;
    let y = cursor.row as f32 * metrics.cell_height;

    let (pos_x, pos_y, w, h) = match cursor.shape {
        CursorShape::Block => (x, y, metrics.cell_width, metrics.cell_height),
        CursorShape::Underline => {
            let underline_height = (metrics.cell_height * 0.15).max(2.0);
            (
                x,
                y + metrics.cell_height - underline_height,
                metrics.cell_width,
                underline_height,
            )
        }
        CursorShape::Bar => {
            let bar_width = (metrics.cell_width * 0.15).max(2.0);
            (x, y, bar_width, metrics.cell_height)
        }
    };

    build_quad_vertices(pos_x, pos_y, w, h, color).to_vec()
}

/// WGPU graphics pipeline for rendering solid colored 2D quads.
pub struct QuadPipeline {
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    vertex_buffer: Option<wgpu::Buffer>,
    vertex_capacity: usize,
}

impl QuadPipeline {
    /// Creates a new quad rendering pipeline for the specified texture format.
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("quad-shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("shaders/quad.wgsl"))),
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("quad-uniform-buffer"),
            size: 8, // 2 x f32 screen size
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("quad-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("quad-bg"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("quad-pipeline-layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: 24, // 2 x f32 position + 4 x f32 color = 24 bytes
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 8,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        };

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("quad-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(vertex_layout)],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
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
            uniform_buffer,
            bind_group,
            vertex_buffer: None,
            vertex_capacity: 0,
        }
    }

    /// Renders vertices to the active render pass.
    pub fn draw<'a>(
        &'a mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        rpass: &mut wgpu::RenderPass<'a>,
        screen_size: [f32; 2],
        vertices: &[QuadVertex],
    ) {
        if vertices.is_empty() {
            return;
        }

        // Upload screen dimensions uniform
        let mut raw_uniform = [0u8; 8];
        raw_uniform[0..4].copy_from_slice(&screen_size[0].to_ne_bytes());
        raw_uniform[4..8].copy_from_slice(&screen_size[1].to_ne_bytes());
        queue.write_buffer(&self.uniform_buffer, 0, &raw_uniform);

        // Resize vertex buffer if needed
        if self.vertex_buffer.is_none() || self.vertex_capacity < vertices.len() {
            let capacity = vertices.len().next_power_of_two().max(64);
            let byte_size = (capacity * std::mem::size_of::<QuadVertex>()) as u64;
            self.vertex_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("quad-vertex-buffer"),
                size: byte_size,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            self.vertex_capacity = capacity;
        }

        let vb = self
            .vertex_buffer
            .as_ref()
            .expect("vertex buffer allocated");
        let vertex_bytes = vertices_to_bytes(vertices);
        queue.write_buffer(vb, 0, &vertex_bytes);

        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, &self.bind_group, &[]);
        rpass.set_vertex_buffer(0, vb.slice(..));
        let count = u32::try_from(vertices.len()).unwrap_or(u32::MAX);
        rpass.draw(0..count, 0..1);
    }
}

use std::hash::{DefaultHasher, Hash, Hasher};

/// A cached shaped text row buffer with its content hash signature.
struct CachedRowBuffer {
    signature: u64,
    buffer: FontBuffer,
}

/// Cache of shaped text buffers across terminal rows to avoid reshapes on unchanged lines.
#[derive(Default)]
pub struct GlyphCache {
    rows: Vec<Option<CachedRowBuffer>>,
}

impl GlyphCache {
    /// Creates a new empty glyph cache.
    #[must_use]
    pub fn new() -> Self {
        Self { rows: Vec::new() }
    }

    /// Resizes the cache entry storage to accommodate the visible grid rows.
    pub fn resize(&mut self, row_count: usize) {
        self.rows.resize_with(row_count, || None);
    }

    /// Clears all cached row buffers.
    pub fn clear(&mut self) {
        self.rows.clear();
    }

    /// Returns the number of cached row entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Returns whether the cache is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

/// WGPU graphics pipeline for rendering text glyphs via glyphon.
pub struct GlyphPipeline {
    /// Font system containing loaded font faces.
    pub font_system: FontSystem,
    /// Rasterization swash cache.
    pub swash_cache: SwashCache,
    /// Viewport for projecting text into window coordinates.
    pub viewport: Viewport,
    /// Texture atlas for glyph caching on the GPU.
    pub atlas: TextAtlas,
    /// Text renderer for drawing text quads.
    pub text_renderer: TextRenderer,
    /// Cache of shaped row text buffers.
    pub glyph_cache: GlyphCache,
}

impl GlyphPipeline {
    /// Creates a new glyph pipeline for the target surface texture format.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let font_system = FontSystem::new();
        let swash_cache = SwashCache::new();
        let cache = Cache::new(device);
        let viewport = Viewport::new(device, &cache);
        let mut atlas = TextAtlas::new(device, queue, &cache, format);
        let text_renderer =
            TextRenderer::new(&mut atlas, device, wgpu::MultisampleState::default(), None);
        let glyph_cache = GlyphCache::new();

        Self {
            font_system,
            swash_cache,
            viewport,
            atlas,
            text_renderer,
            glyph_cache,
        }
    }
}

#[inline]
#[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
fn float_to_u8(val: f32) -> u8 {
    (val.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Context parameters required for drawing operations.
pub struct DrawContext<'a> {
    /// Logical GPU device.
    pub device: &'a wgpu::Device,
    /// Command execution queue.
    pub queue: &'a wgpu::Queue,
    /// Screen dimensions in physical pixels [width, height].
    pub screen_size: [u32; 2],
    /// Monospace cell dimensions and font metrics.
    pub metrics: &'a CellMetrics,
    /// 16-color ANSI palette.
    pub palette: &'a [[f32; 4]; 16],
}

fn collect_single_row_spans(
    row_cells: &[crate::grid::Cell],
    palette: &[[f32; 4]; 16],
) -> Vec<(String, FontAttrs<'static>)> {
    let Some(last_col) = row_cells.iter().rposition(|c| c.ch != ' ') else {
        return Vec::new();
    };

    let mut spans: Vec<(String, FontAttrs<'static>)> = Vec::new();
    let mut current_text = String::new();
    let mut current_style: Option<(GlyphonColor, bool, bool)> = None;

    for cell in &row_cells[..=last_col] {
        if cell.width == 0 {
            continue;
        }

        let fg_rgba = color_to_rgba(cell.fg, [0.9, 0.9, 0.9, 1.0], palette);
        let color = GlyphonColor::rgba(
            float_to_u8(fg_rgba[0]),
            float_to_u8(fg_rgba[1]),
            float_to_u8(fg_rgba[2]),
            255,
        );
        let is_bold = cell.attrs.contains(crate::grid::Attrs::BOLD);
        let is_italic = cell.attrs.contains(crate::grid::Attrs::ITALIC);
        let cell_style = (color, is_bold, is_italic);

        if current_style == Some(cell_style) {
            current_text.push(cell.ch);
        } else {
            if !current_text.is_empty() {
                if let Some((c, bold, italic)) = current_style {
                    let mut attrs = FontAttrs::new().family(Family::Monospace).color(c);
                    if bold {
                        attrs = attrs.weight(Weight::BOLD);
                    }
                    if italic {
                        attrs = attrs.style(Style::Italic);
                    }
                    spans.push((current_text, attrs));
                }
                current_text = String::new();
            }
            current_style = Some(cell_style);
            current_text.push(cell.ch);
        }
    }

    if !current_text.is_empty()
        && let Some((c, bold, italic)) = current_style
    {
        let mut attrs = FontAttrs::new().family(Family::Monospace).color(c);
        if bold {
            attrs = attrs.weight(Weight::BOLD);
        }
        if italic {
            attrs = attrs.style(Style::Italic);
        }
        spans.push((current_text, attrs));
    }

    spans
}

#[cfg(test)]
fn collect_row_spans(
    grid: &Grid,
    palette: &[[f32; 4]; 16],
) -> Vec<(usize, Vec<(String, FontAttrs<'static>)>)> {
    let mut row_spans = Vec::new();
    for row_idx in 0..grid.rows {
        if let Some(row_cells) = grid.display_row(row_idx) {
            let spans = collect_single_row_spans(row_cells, palette);
            if !spans.is_empty() {
                row_spans.push((row_idx, spans));
            }
        }
    }
    row_spans
}

impl GlyphPipeline {
    /// Formats grid characters and renders text glyphs into the active render pass.
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    pub fn draw<'pass>(
        &'pass mut self,
        ctx: &DrawContext<'_>,
        rpass: &mut wgpu::RenderPass<'pass>,
        grid: &Grid,
    ) -> Result<(), glyphon::RenderError> {
        self.glyph_cache.resize(grid.rows);

        for row_idx in 0..grid.rows {
            let Some(row_cells) = grid.display_row(row_idx) else {
                self.glyph_cache.rows[row_idx] = None;
                continue;
            };

            if !row_cells.iter().any(|c| c.ch != ' ') {
                self.glyph_cache.rows[row_idx] = None;
                continue;
            }

            let mut hasher = DefaultHasher::new();
            row_idx.hash(&mut hasher);
            ctx.metrics.font_size.to_bits().hash(&mut hasher);
            ctx.metrics.line_height.to_bits().hash(&mut hasher);
            row_cells.hash(&mut hasher);
            let signature = hasher.finish();

            let needs_update = match &self.glyph_cache.rows[row_idx] {
                Some(cached) => cached.signature != signature,
                None => true,
            };

            if needs_update {
                let spans = collect_single_row_spans(row_cells, ctx.palette);
                let mut buffer = match self.glyph_cache.rows[row_idx].take() {
                    Some(mut c) => {
                        c.buffer.set_metrics(Metrics::new(
                            ctx.metrics.font_size,
                            ctx.metrics.line_height,
                        ));
                        c.buffer
                    }
                    None => FontBuffer::new(
                        &mut self.font_system,
                        Metrics::new(ctx.metrics.font_size, ctx.metrics.line_height),
                    ),
                };
                let rich_spans: Vec<(&str, FontAttrs)> = spans
                    .iter()
                    .map(|(text, attrs)| (text.as_str(), attrs.clone()))
                    .collect();
                buffer.set_rich_text(
                    rich_spans,
                    &FontAttrs::new().family(Family::Monospace),
                    Shaping::Advanced,
                    None,
                );
                buffer.shape_until_scroll(&mut self.font_system, false);
                self.glyph_cache.rows[row_idx] = Some(CachedRowBuffer { signature, buffer });
            }
        }

        let max_w = i32::try_from(ctx.screen_size[0]).unwrap_or(i32::MAX);
        let max_h = i32::try_from(ctx.screen_size[1]).unwrap_or(i32::MAX);
        let text_areas: Vec<TextArea> = self
            .glyph_cache
            .rows
            .iter()
            .enumerate()
            .filter_map(|(row_idx, entry)| {
                let cached = entry.as_ref()?;
                let top = row_idx as f32 * ctx.metrics.cell_height;
                Some(TextArea {
                    buffer: &cached.buffer,
                    left: 0.0,
                    top,
                    scale: 1.0,
                    bounds: TextBounds {
                        left: 0,
                        top: 0,
                        right: max_w,
                        bottom: max_h,
                    },
                    default_color: GlyphonColor::rgb(255, 255, 255),
                    custom_glyphs: &[],
                })
            })
            .collect();

        self.viewport.update(
            ctx.queue,
            Resolution {
                width: ctx.screen_size[0],
                height: ctx.screen_size[1],
            },
        );

        let _ = self.text_renderer.prepare(
            ctx.device,
            ctx.queue,
            &mut self.font_system,
            &mut self.atlas,
            &self.viewport,
            text_areas,
            &mut self.swash_cache,
        );

        self.text_renderer
            .render(&self.atlas, &self.viewport, rpass)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).abs() < f32::EPSILON && (a[1] - b[1]).abs() < f32::EPSILON
    }

    fn approx_eq4(a: [f32; 4], b: [f32; 4]) -> bool {
        (a[0] - b[0]).abs() < f32::EPSILON
            && (a[1] - b[1]).abs() < f32::EPSILON
            && (a[2] - b[2]).abs() < f32::EPSILON
            && (a[3] - b[3]).abs() < f32::EPSILON
    }

    #[test]
    fn build_quad_vertices_creates_two_triangles() {
        let quad = build_quad_vertices(10.0, 20.0, 8.0, 16.0, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(quad.len(), 6);
        assert!(approx_eq(quad[0].position, [10.0, 20.0]));
        assert!(approx_eq(quad[5].position, [18.0, 36.0]));
    }

    #[test]
    fn color_to_rgba_converts_correctly() {
        let default_c = [0.1, 0.1, 0.1, 1.0];
        assert!(approx_eq4(
            color_to_rgba(Color::Default, default_c, &DEFAULT_PALETTE),
            default_c
        ));
        assert!(approx_eq4(
            color_to_rgba(Color::Rgb(255, 0, 0), default_c, &DEFAULT_PALETTE),
            [1.0, 0.0, 0.0, 1.0]
        ));
        assert!(approx_eq4(
            color_to_rgba(Color::Indexed(1), default_c, &DEFAULT_PALETTE),
            DEFAULT_PALETTE[1]
        ));
    }

    #[test]
    fn build_cell_background_vertices_filters_default_bg() {
        let mut grid = Grid::new(5, 5);
        let verts_empty = build_cell_background_vertices(&grid, 10.0, 20.0, &DEFAULT_PALETTE);
        assert_eq!(verts_empty, Vec::<QuadVertex>::new());

        grid[0][0].bg = Color::Indexed(1);
        let verts_one = build_cell_background_vertices(&grid, 10.0, 20.0, &DEFAULT_PALETTE);
        assert_eq!(verts_one.len(), 6);
    }

    #[test]
    fn glyph_buffer_shaping_handles_spans() {
        let mut font_system = FontSystem::new();
        let mut buffer = FontBuffer::new(&mut font_system, Metrics::new(14.0, 16.0));
        let spans = [
            (
                "Hello ",
                FontAttrs::new()
                    .family(Family::Monospace)
                    .color(GlyphonColor::rgb(255, 100, 0)),
            ),
            (
                "World",
                FontAttrs::new()
                    .family(Family::Monospace)
                    .color(GlyphonColor::rgb(0, 200, 255)),
            ),
        ];
        buffer.set_rich_text(
            spans,
            &FontAttrs::new().family(Family::Monospace),
            Shaping::Advanced,
            None,
        );
        buffer.shape_until_scroll(&mut font_system, false);
        assert!(!buffer.lines.is_empty());
    }

    #[test]
    fn build_cursor_vertices_shapes_and_visibility() {
        let metrics = CellMetrics {
            cell_width: 10.0,
            cell_height: 20.0,
            font_size: 14.0,
            line_height: 20.0,
        };
        let color = [1.0, 1.0, 1.0, 1.0];

        // 1. Invisible cursor returns empty
        let mut cursor = Cursor {
            row: 1,
            col: 2,
            visible: false,
            shape: CursorShape::Block,
        };
        assert_eq!(
            build_cursor_vertices(&cursor, &metrics, color),
            Vec::<QuadVertex>::new()
        );

        // 2. Visible block cursor
        cursor.visible = true;
        let block_verts = build_cursor_vertices(&cursor, &metrics, color);
        assert_eq!(block_verts.len(), 6);
        assert!(approx_eq(block_verts[0].position, [20.0, 20.0]));
        assert!(approx_eq(block_verts[5].position, [30.0, 40.0]));

        // 3. Visible underline cursor
        cursor.shape = CursorShape::Underline;
        let underline_verts = build_cursor_vertices(&cursor, &metrics, color);
        assert_eq!(underline_verts.len(), 6);
        assert!(approx_eq(underline_verts[0].position, [20.0, 37.0])); // 20 + 20 - 3.0

        // 4. Visible bar cursor
        cursor.shape = CursorShape::Bar;
        let bar_verts = build_cursor_vertices(&cursor, &metrics, color);
        assert_eq!(bar_verts.len(), 6);
        assert!(approx_eq(bar_verts[5].position, [22.0, 40.0])); // 20 + 2.0 width
    }

    #[test]
    fn glyph_cache_lifecycle_and_invalidation() {
        let mut cache = GlyphCache::new();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);

        cache.resize(24);
        assert_eq!(cache.len(), 24);
        assert!(!cache.is_empty());

        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn collect_row_spans_applies_bold_and_italic_styles() {
        let mut grid = Grid::new(10, 1);
        grid[0][0] = crate::grid::Cell::new(
            'B',
            Color::Default,
            Color::Default,
            crate::grid::Attrs::BOLD,
            1,
        );
        grid[0][1] = crate::grid::Cell::new(
            'I',
            Color::Default,
            Color::Default,
            crate::grid::Attrs::ITALIC,
            1,
        );
        grid[0][2] = crate::grid::Cell::new(
            'X',
            Color::Default,
            Color::Default,
            crate::grid::Attrs::BOLD | crate::grid::Attrs::ITALIC,
            1,
        );

        let spans = collect_row_spans(&grid, &DEFAULT_PALETTE);
        assert_eq!(spans.len(), 1);
        let row_spans = &spans[0].1;
        assert_eq!(row_spans.len(), 3);

        assert_eq!(row_spans[0].0, "B");
        assert_eq!(row_spans[0].1.weight, Weight::BOLD);
        assert_eq!(row_spans[0].1.style, Style::Normal);

        assert_eq!(row_spans[1].0, "I");
        assert_eq!(row_spans[1].1.weight, Weight::NORMAL);
        assert_eq!(row_spans[1].1.style, Style::Italic);

        assert_eq!(row_spans[2].0, "X");
        assert_eq!(row_spans[2].1.weight, Weight::BOLD);
        assert_eq!(row_spans[2].1.style, Style::Italic);
    }
}
