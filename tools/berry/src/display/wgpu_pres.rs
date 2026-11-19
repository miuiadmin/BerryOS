//! wgpu 呈现骨架（B1-b 壳半第一刀）——Metal 后端 GPU 合成替代 softbuffer CPU 呈现。
//!
//! 依据链：13 号 §2 排刀第 1 刀（B1-b 壳半 = β-0 收尾）——「宿主 GPU 参与合成」；
//! 15 号 PoC 实测定谳：直配 `write_texture`（bpr=stride、任意非对齐值合法——
//! wgpu-types texture.rs:844 验证层不执法 256/512 对齐）为生产形态，damage
//! K=8×(16×10)/帧 p50 25µs（0.15% 帧预算）、全帧 1080×1920 最坏 606–650µs
//! （3.9% 预算）；行重排无收益（内部 chunked 逐行重排自带）。
//!
//! 呈现链：共享图替身/VNC 帧写入 GPU 纹理 → 采样 blit（最近/线性）→ surface。
//! softbuffer 路径保留为 fallback（wgpu 适配器缺席时自动降级，零行为漂移）。

use crate::display::frame::Frame;
use std::sync::Arc;

/// 单帧 damage 矩形上限（guifbd DAMAGE_MAX 同值，user/berry-ui/src/damage.rs K≤8）。
pub const DAMAGE_MAX: usize = 8;

/// 一帧的增量载荷：脏矩形集 + 各自在共享图中的偏移。
/// 消费者（VNC FrameUpdate / β-1 UpdateMap）产矩形流，本骨架按 15 号 §2.2
/// 直配切片上传（每矩形独立 write_texture，~3µs/矩形含全部框架费）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DamageRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// wgpu 呈现器。生命周期 = 窗口会话；纹理按帧几何 (w,h) 惰性重建。
pub struct WgpuPresenter {
    pub adapter: String,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    surface: wgpu::Surface<'static>,
    /// 客帧纹理（Bgra8Unorm，guest 像素直配）。
    texture: Option<wgpu::Texture>,
    tex_size: (u32, u32),
    /// 交互层 overlay 纹理（窗口尺寸，Rgba8——alpha 0=透客帧/255=覆盖，
    /// 工具栏/菜单/tooltip CPU 绘制后整层上传，shader 内 alpha 合成）。
    overlay: Option<wgpu::Texture>,
    overlay_size: (u32, u32),
    sampler: wgpu::Sampler,
    /// blit 管线（WGSL：采样 + alpha 强制 1.0——15 号 §2.3 同款）。
    pipeline: wgpu::RenderPipeline,
    bind_group: Option<wgpu::BindGroup>,
}

impl WgpuPresenter {
    /// 初始化 wgpu 面。失败返回 None（调用方降级 softbuffer——零行为漂移）。
    pub fn new(window: Arc<winit::window::Window>) -> Option<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::METAL,
            flags: wgpu::InstanceFlags::default(),
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            backend_options: wgpu::BackendOptions::default(),
            display: None,
        });
        let surface = instance.create_surface(window.clone()).ok()?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .ok()?;
        let adapter_name = adapter.get_info().name.clone();
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("berry-wgpu"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            ..Default::default()
        }))
        .ok()?;
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| matches!(f, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb))
            .unwrap_or(caps.formats[0]);
        let size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        surface.configure(&device, &config);

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("berry-blit-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("berry-blit"),
            source: wgpu::ShaderSource::Wgsl(SHADER_BLIT.into()),
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("berry-blit-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("berry-blit-pl"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("berry-blit-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(config.format.into())],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        Some(Self {
            adapter: adapter_name,
            device,
            queue,
            config,
            surface,
            texture: None,
            tex_size: (0, 0),
            overlay: None,
            overlay_size: (0, 0),
            sampler,
            pipeline,
            bind_group: None,
        })
    }

    /// 全帧上传（guest 帧直配——bpr=stride 任意值，15 号 §2.4 实测合法）。
    pub fn upload_full(&mut self, frame: &Frame) {
        let (w, h) = (frame.width, frame.height);
        if w == 0 || h == 0 {
            return;
        }
        self.ensure_texture(w, h);
        let Some(tex) = self.texture.as_ref() else { return };
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(&frame.pixels),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(frame.width * 4),
                rows_per_image: None,
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        self.rebuild_bind_group();
    }

    /// 交互层整层上传（窗口尺寸 overlay——CPU 绘制函数产 0x00RRGGBB u32 面，
    /// alpha 通道由调用方写 0/255；未写 = 0 全透）。`rgba` 长度须 = w*h*4。
    pub fn upload_overlay(&mut self, rgba: &[u8], w: u32, h: u32) {
        if w == 0 || h == 0 || rgba.len() < (w as usize) * (h as usize) * 4 {
            return;
        }
        self.ensure_overlay(w, h);
        let Some(ov) = self.overlay.as_ref() else { return };
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: ov,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: None,
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        // overlay 尺寸变化时 bind group 重建（纹理对象换了）。
    }

    /// damage 增量上传（K≤8 矩形/帧——每矩形独立直配切片，~3µs/矩形）。
    /// `data` = 共享图替身（整图在内存，VNC 帧缓冲同形态）；矩形坐标为客坐标系。
    pub fn upload_damage(&mut self, data: &[u8], stride: u32, rects: &[DamageRect]) {
        if rects.is_empty() {
            return;
        }
        let Some(tex) = self.texture.as_ref() else { return };
        for r in rects.iter().take(DAMAGE_MAX) {
            let (x, y, w, h) = (r.x, r.y, r.w.min(1 << 14), r.h.min(1 << 14));
            if w == 0 || h == 0 {
                continue;
            }
            let row_start = y as usize * stride as usize + x as usize * 4;
            let row_bytes = w as usize * 4;
            // 每矩形一次 write_texture：内部 chunked 逐行重排（15 号 §2.2 判读 A 路）。
            for row in 0..h as usize {
                let off = row_start + row * stride as usize;
                let end = off + row_bytes;
                if end > data.len() {
                    break;
                }
                self.queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: tex,
                        mip_level: 0,
                        origin: wgpu::Origin3d { x, y: y + row as u32, z: 0 },
                        aspect: wgpu::TextureAspect::All,
                    },
                    &data[off..end],
                    wgpu::TexelCopyBufferLayout {
                        offset:  0,
                        bytes_per_row: Some(row_bytes as u32),
                        rows_per_image: None,
                    },
                    wgpu::Extent3d { width: w, height: 1, depth_or_array_layers: 1 },
                );
            }
        }
    }

    /// 呈现一帧（blit 到 surface + present）。窗口几何变化时先 reconfigure。
    pub fn present(&mut self, window: &winit::window::Window) -> bool {
        let size = window.inner_size();
        let (w, h) = (size.width.max(1), size.height.max(1));
        if w != self.config.width || h != self.config.height {
            self.config.width = w;
            self.config.height = h;
            self.surface.configure(&self.device, &self.config);
        }
        // overlay 与 surface 同尺寸（窗口几何变化即重建）。
        if self.overlay_size != (w, h) {
            self.ensure_overlay(w, h);
        }
        let Some(bind) = self.bind_group.as_ref() else { return false };
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) => f,
            wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            _ => return false,
        };
        let view = frame.texture.create_view(&Default::default());
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("berry-blit-enc") });
        let mut rpass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("berry-blit-rpass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, bind, &[]);
        rpass.draw(0..3, 0..1);
        drop(rpass);
        self.queue.submit([enc.finish()]);
        self.queue.present(frame);
        true
    }

    pub fn texture_size(&self) -> (u32, u32) {
        self.tex_size
    }

    fn ensure_overlay(&mut self, w: u32, h: u32) {
        if self.overlay_size == (w, h) && self.overlay.is_some() {
            return;
        }
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("berry-overlay-tex"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.overlay = Some(tex);
        self.overlay_size = (w, h);
        self.bind_group = None;
    }

    fn ensure_texture(&mut self, w: u32, h: u32) {
        if self.tex_size == (w, h) && self.texture.is_some() {
            return;
        }
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("berry-guest-tex"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.texture = Some(tex);
        self.tex_size = (w, h);
        self.bind_group = None;
    }

    fn rebuild_bind_group(&mut self) {
        if self.bind_group.is_some() {
            return;
        }
        let (Some(tex), Some(ov)) = (self.texture.as_ref(), self.overlay.as_ref()) else { return };
        let bgl = self.pipeline.get_bind_group_layout(0);
        self.bind_group = Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("berry-blit-bind"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&tex.create_view(&Default::default())),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(
                        &ov.create_view(&Default::default()),
                    ),
                },
            ],
        }));
    }
}

const SHADER_BLIT: &str = r#"
struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VsOut {
    // 全屏三角形（blit 不需要顶点缓冲）；uv y 翻转（纹理 v=0 在顶）。
    var p = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -3.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 3.0,  1.0),
    );
    var u = array<vec2<f32>, 3>(
        vec2<f32>(0.0, 2.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(2.0, 0.0),
    );
    var out: VsOut;
    out.pos = vec4<f32>(p[vi], 0.0, 1.0);
    out.uv = u[vi];
    return out;
}

@group(0) @binding(0) var samp: sampler;
@group(0) @binding(1) var tex: texture_2d<f32>;

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    var c = textureSample(tex, samp, in.uv);
    c.a = 1.0; // alpha 强制 1.0（15 号 §2.3 同款——X 通道未定义）
    return c;
}
"#;
