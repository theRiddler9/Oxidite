//! Cell backgrounds, glyphs, and cursor drawing.

use crate::grid::{Color, Grid};
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
        for col in 0..grid.cols {
            let cell = &grid[row][col];
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
        assert!(verts_empty.is_empty());

        grid[0][0].bg = Color::Indexed(1);
        let verts_one = build_cell_background_vertices(&grid, 10.0, 20.0, &DEFAULT_PALETTE);
        assert_eq!(verts_one.len(), 6);
    }
}
