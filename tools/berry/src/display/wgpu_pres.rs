//! wgpu 呈现骨架（B1-b 壳半第一刀）——Metal 后端 GPU 合成替代 softbuffer CPU 呈现。
//!
//! 依据链：13 号 §2 排刀第 1 刀（B1-b 壳半 = β-0 收尾）——「宿主 GPU 参与合成」；
//! 15 号 PoC 实测定谳：直配 `write_texture`（bpr=stride、任意非对齐值合法——
//! wgpu-types texture.rs:844 验证层不执法 256/512 对齐）为生产形态，damage
//! K=8×(16×10)/帧 p50 25µs（0.15% 帧预算）、全帧 1080×1920 最坏 606–650µs
//! （3.9% 预算）；行重排无收益（内部 chunked 逐行重排自带）。
//!
//! 呈现链：共享图替身/VNC 帧写入 GPU 纹理 → letterbox 缩放采样（目标矩形
//! 经 uniform 下发，scale::fit 单源几何）→ overlay alpha 合成 → surface。
//! softbuffer 路径保留为 fallback（wgpu 适配器缺席时自动降级，零行为漂移）。

use crate::display::frame::Frame;
use std::sync::Arc;

/// 单帧 damage 矩形上限（guifbd DAMAGE_MAX 同值，user/berry-ui/src/damage.rs K≤8）。
pub const DAMAGE_MAX: usize = 8;

/// 采样器 + blit 管线 + 几何 uniform 缓冲构建（new 与 headless 静态验证
/// 测试共用单源——WGSL 语法/BGL 绑定错配在此即被 wgpu/naga 校验层拦截）。
fn build_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) -> (wgpu::Sampler, wgpu::RenderPipeline, wgpu::Buffer) {
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
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(64),
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
            targets: &[Some(format.into())],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });
    let params_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("berry-blit-params"),
        size: 64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    (sampler, pipeline, params_buf)
}

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
    /// blit 管线（WGSL：letterbox 采样客帧 + overlay alpha 合成）。
    pipeline: wgpu::RenderPipeline,
    bind_group: Option<wgpu::BindGroup>,
    /// letterbox/overlay 几何 uniform（target rect + surface 尺寸，每帧直写）。
    params_buf: wgpu::Buffer,
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

        let (sampler, pipeline, params_buf) = build_pipeline(&device, config.format);

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
            params_buf,
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
        // overlay 尺寸变化换了纹理对象：绑定组同步重建（否则 present 裸拿
        // 旧组或 None，窗口 resize 后首帧错误降级 softbuffer）。
        self.rebuild_bind_group();
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

    /// 呈现一帧（blit 到 surface + present）。`rect` = 客帧 letterbox 目标
    /// 矩形（窗口像素坐标 x,y,w,h——scale::fit 单源产物，与 softbuffer 段
    /// fit 几何语义一致，pointer 映射同源）。窗口几何变化时先 reconfigure。
    pub fn present(&mut self, window: &winit::window::Window, rect: (u32, u32, u32, u32)) -> bool {
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
            self.rebuild_bind_group();
        }
        // letterbox/overlay 几何 uniform（2×vec4：target rect + surface 尺寸）。
        let params: [[f32; 4]; 2] = [
            [rect.0 as f32, rect.1 as f32, rect.2 as f32, rect.3 as f32],
            [w as f32, h as f32, 0.0, 0.0],
        ];
        self.queue
            .write_buffer(&self.params_buf, 0, bytemuck::cast_slice(&params));
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
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.params_buf.as_entire_binding(),
                },
            ],
        }));
    }
}

const SHADER_BLIT: &str = r#"
struct Params {
    /// 客帧 letterbox 目标矩形（窗口像素，左上原点）：x, y, w, h。
    rect: vec4<f32>,
    /// surface 尺寸（窗口像素）：w, h（zw 保留）。
    surface: vec4<f32>,
};

@group(0) @binding(0) var samp: sampler;
@group(0) @binding(1) var tex: texture_2d<f32>;
@group(0) @binding(2) var overlay: texture_2d<f32>;
@group(0) @binding(3) var<uniform> params: Params;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    // 全屏三角形（覆盖整个 surface；letterbox 与采样均按片元坐标在 fs 算）。
    var p = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -3.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 3.0,  1.0),
    );
    return vec4<f32>(p[vi], 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    // 片元坐标（像素，左上原点；pos.xy 含 0.5 像素中心偏移，天然对心采样）。
    let px = pos.xy;
    let r = params.rect;
    // 客帧采样：目标矩形内线性缩放；clamp + max 防退化矩形除零（矩形外
    // 采样值不进合成）。textureSample 须在统一控制流——分支压到 select。
    let guv = clamp((px - r.xy) / max(r.zw, vec2<f32>(1.0, 1.0)), vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0));
    var c = textureSample(tex, samp, guv);
    c.a = 1.0; // alpha 强制 1.0（15 号 §2.3 同款——X 通道未定义）
    // overlay 采样：窗口全幅（工具栏/菜单/tooltip 按屏幕坐标覆盖）。
    let ouv = clamp(px / max(params.surface.xy, vec2<f32>(1.0, 1.0)), vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0));
    let o = textureSample(overlay, samp, ouv);
    // letterbox 外底色 0x0010_1418（softbuffer 段 scale::render 填充色同源）；
    // overlay 在底色之上 straight-alpha 合成（alpha 0=透底/客帧，255=覆盖）。
    let bg = vec3<f32>(0.0627451, 0.07843137, 0.09411765);
    let inside = px.x >= r.x && px.y >= r.y && px.x < r.x + r.z && px.y < r.y + r.w;
    let base = select(bg, c.rgb, inside);
    return vec4<f32>(mix(base, o.rgb, o.a), 1.0);
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    /// headless 静态验证：无窗口建 Metal 设备过一遍采样器/shader/BGL/管线
    /// 构建（letterbox+overlay 改刀守护网——WGSL 语法或绑定错配在校验层
    /// 即挂红，不必等原生窗口首帧）。无 Metal 宿主跳过（不挂红）。
    #[test]
    fn shader_and_pipeline_validate_headless() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::METAL,
            flags: wgpu::InstanceFlags::default(),
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            backend_options: wgpu::BackendOptions::default(),
            display: None,
        });
        let adapter = match pollster::block_on(instance.request_adapter(&Default::default())) {
            Ok(a) => a,
            Err(_) => return,
        };
        let device = match pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("berry-wgpu-test"),
            ..Default::default()
        })) {
            Ok((device, _queue)) => device,
            Err(_) => return,
        };
        let (_sampler, _pipeline, _params) =
            build_pipeline(&device, wgpu::TextureFormat::Bgra8Unorm);
    }
}
