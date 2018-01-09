use font::GenericFont;
use window::VkWindow;
use drawcalls::DrawCall;
use drawcalls::DrawMath;
use graphics::CoreRender;
use settings::Settings;
use model_data;

use image;
use winit;

use vulkano::image as vkimage;
use vulkano::sampler;

use vulkano::sync::now;
use vulkano::sync::GpuFuture;
use vulkano::sync::NowFuture;

use vulkano::swapchain;
use vulkano::swapchain::AcquireError;

use vulkano::buffer::cpu_pool;
use vulkano::buffer::BufferUsage;
use vulkano::buffer::BufferAccess;
use vulkano::buffer::CpuAccessibleBuffer;
use vulkano::buffer::ImmutableBuffer;

use vulkano::framebuffer;
use vulkano::framebuffer::RenderPassAbstract;

use vulkano::command_buffer::CommandBufferExecFuture;
use vulkano::command_buffer::DynamicState;
use vulkano::command_buffer::AutoCommandBuffer;
use vulkano::command_buffer::AutoCommandBufferBuilder;

use vulkano::pipeline;
use vulkano::pipeline::viewport::Viewport;
use vulkano::pipeline::GraphicsPipelineAbstract;

use vulkano::format;
use vulkano::image::ImmutableImage;
use vulkano::descriptor::descriptor_set;
use vulkano::swapchain::SwapchainCreationError;

use std::mem;
use std::time;
use std::iter;
use std::slice;
use std::f32::consts;
use std::marker::Sync;
use std::marker::Send;
use std::collections::HashMap;
use std::sync::Arc;

use cgmath;
use cgmath::Vector2;
use cgmath::Vector3;
use cgmath::Matrix4;
use cgmath::SquareMatrix;

#[derive(Debug, Clone)]
struct Vertex { position: [f32; 2], uv: [f32; 2] }

impl_vertex!(Vertex, position, uv);

mod vs_texture {
  #[derive(VulkanoShader)]
  #[ty = "vertex"]
  #[path = "src/shaders/VkTexture.vert"]
  struct Dummy;
}

mod fs_texture {
  #[derive(VulkanoShader)]
  #[ty = "fragment"]
  #[path = "src/shaders/VkTexture.frag"]
  struct Dummy;
}

mod vs_text {
  #[derive(VulkanoShader)]
  #[ty = "vertex"]
  #[path = "src/shaders/VkText.vert"]
  struct Dummy;
}

mod fs_text {
  #[derive(VulkanoShader)]
  #[ty = "fragment"]
  #[path = "src/shaders/VkText.frag"]
  struct Dummy;
}

mod vs_3d {
  #[derive(VulkanoShader)]
  #[ty = "vertex"]
  #[path = "src/shaders/Vk3D.vert"]
  struct Dummy;
}

mod fs_3d {
  #[derive(VulkanoShader)]
  #[ty = "fragment"]
  #[path = "src/shaders/Vk3D.frag"]
  struct Dummy;
}

#[derive(Clone)]
pub struct Model_Info {
  location: String,
  texture: String,
}

pub struct Model {
  vertex_buffer: Vec<Arc<BufferAccess + Send + Sync>>,
  index_buffer: Arc<ImmutableBuffer<[u16]>>,
}

pub struct RawVk {
  ready: bool,
  fonts: HashMap<String, GenericFont>,
  textures: HashMap<String, Arc<ImmutableImage<format::R8G8B8A8Unorm>>>,
  texture_paths: HashMap<String, String>,
  model_paths: HashMap<String, Model_Info>,
  
  framebuffers: Option<Vec<Arc<framebuffer::FramebufferAbstract + Send + Sync>>>,
  render_pass: Option<Arc<RenderPassAbstract + Send + Sync>>,

  depth_buffer: Option<Arc<vkimage::AttachmentImage<format::D16Unorm>>>,
  
  //3D
  models: HashMap<String, Model>,
  
  pipeline_3d: Option<Arc<GraphicsPipelineAbstract + Send + Sync>>,
  
  projection_3d: Matrix4<f32>,
  view: Matrix4<f32>,
  scale: Matrix4<f32>,
  
  uniform_buffer_3d: cpu_pool::CpuBufferPool<vs_3d::ty::Data>,

  //2D
  vertex_buffer_2d: Option<Vec<Arc<BufferAccess + Send + Sync>>>,
  index_buffer_2d: Option<Arc<ImmutableBuffer<[u16]>>>,

  pipeline_text: Option<Arc<GraphicsPipelineAbstract + Send + Sync>>,
  pipeline_texture: Option<Arc<GraphicsPipelineAbstract + Send + Sync>>,

  projection_2d: Matrix4<f32>,

  uniform_buffer_texture: cpu_pool::CpuBufferPool<vs_texture::ty::Data>,
  uniform_buffer_text: cpu_pool::CpuBufferPool<vs_text::ty::Data>,

  // Vk System stuff
  pub window: VkWindow,
  sampler: Arc<sampler::Sampler>,

  recreate_swapchain: bool,
  
  previous_frame_end: Option<Box<GpuFuture>>,
}

impl RawVk {
  pub fn new() -> RawVk {
    let mut settings = Settings::load();
    let width = settings.get_resolution()[0];
    let height = settings.get_resolution()[1];
    let min_width = settings.get_minimum_resolution()[0];
    let min_height = settings.get_minimum_resolution()[1];
    let fullscreen = settings.is_fullscreen();
    
    let window = VkWindow::new(width, height, min_width, min_height, fullscreen);
    
    let proj_2d = Matrix4::identity();
    let proj_3d = Matrix4::identity();
    
    let view = cgmath::Matrix4::look_at(cgmath::Point3::new(0.0, 0.0, -1.0), cgmath::Point3::new(0.0, 0.0, 0.0), cgmath::Vector3::new(0.0, -1.0, 0.0));
    let scale = cgmath::Matrix4::from_scale(0.01);
    
    let sampler = sampler::Sampler::new(window.get_device(), sampler::Filter::Linear,
                                                   sampler::Filter::Linear, 
                                                   sampler::MipmapMode::Nearest,
                                                   sampler::SamplerAddressMode::ClampToEdge,
                                                   sampler::SamplerAddressMode::ClampToEdge,
                                                   sampler::SamplerAddressMode::ClampToEdge,
                                                   0.0, 1.0, 0.0, 0.0).unwrap();
 
    let text_uniform = cpu_pool::CpuBufferPool::new(window.get_device(), BufferUsage::uniform_buffer());
    let texture_uniform = cpu_pool::CpuBufferPool::new(window.get_device(), BufferUsage::uniform_buffer());
    let uniform_3d = cpu_pool::CpuBufferPool::<vs_3d::ty::Data>::new(window.get_device(), BufferUsage::uniform_buffer());
    let previous_frame_end = Some(Box::new(now(window.get_device())) as Box<GpuFuture>);
    
    RawVk {
      ready: false,
      fonts: HashMap::new(),
      textures: HashMap::new(),
      texture_paths: HashMap::new(),
      model_paths: HashMap::new(),

      framebuffers: None,
      render_pass: None,

      depth_buffer: None,

      // 3D
      models: HashMap::new(),
      
      pipeline_3d: None,
      
      projection_3d: proj_3d,
      view: view,
      scale: scale,

      uniform_buffer_3d: uniform_3d,

      //2D
      vertex_buffer_2d: None,
      index_buffer_2d: None,
      
      pipeline_texture: None,
      pipeline_text: None,
      
      projection_2d: proj_2d,
            
      uniform_buffer_texture: texture_uniform,
      uniform_buffer_text: text_uniform,

      // Vk System
      window: window,
      sampler: sampler,

      recreate_swapchain: false,
      
      previous_frame_end: previous_frame_end,
    }
  }
  
  pub fn create_2d_vertex(&self) -> Arc<BufferAccess + Send + Sync> {
    #[derive(Debug, Clone)]
    struct Vertex { position: [f32; 2], uv: [f32; 2] }

    impl_vertex!(Vertex, position, uv);
    
    let square = {
      [
          Vertex { position: [  0.5 ,   0.5 ], uv: [1.0, 0.0] },
          Vertex { position: [ -0.5,    0.5 ], uv: [0.0, 0.0] },
          Vertex { position: [ -0.5,   -0.5 ], uv: [0.0, 1.0] },
          Vertex { position: [  0.5 ,  -0.5 ], uv: [1.0, 1.0] },
      ]
    };
    
    CpuAccessibleBuffer::from_iter(self.window.get_device(), BufferUsage::vertex_buffer(), square.iter().cloned()).expect("failed to create vertex buffer")
  }
  
  pub fn create_2d_index(&self) -> (Arc<ImmutableBuffer<[u16]>>,
                                    CommandBufferExecFuture<NowFuture, AutoCommandBuffer>) {
    let indicies: [u16; 6] = [1, 2, 3, 0, 3, 1];
    
    ImmutableBuffer::from_iter(indicies.iter().cloned(), BufferUsage::index_buffer(), self.window.get_queue()).expect("failed to create immutable index buffer")
  }
  
  pub fn create_vertex(&self, verticies: iter::Cloned<slice::Iter<model_data::Vertex>>) -> Arc<BufferAccess + Send + Sync> {
      CpuAccessibleBuffer::from_iter(self.window.get_device(), BufferUsage::vertex_buffer(), verticies).expect("failed to create vertex buffer")
  }
  
  pub fn create_index(&self, indicies: iter::Cloned<slice::Iter<u16>>) -> (Arc<ImmutableBuffer<[u16]>>,
                                                                           CommandBufferExecFuture<NowFuture, AutoCommandBuffer>) {
      ImmutableBuffer::from_iter(indicies, BufferUsage::index_buffer(), self.window.get_queue()).expect("failed to create immutable teapot index buffer")
  }
  
  pub fn create_2d_projection(&self, width: f32, height: f32) -> Matrix4<f32> {
    cgmath::ortho(0.0, width, height, 0.0, -1.0, 1.0)
  }
  
  pub fn create_3d_projection(&self, width: f32, height: f32) -> Matrix4<f32> {
    cgmath::perspective(cgmath::Rad(consts::FRAC_PI_4), { width as f32 / height as f32 }, 0.01, 100.0)
  }
  
  pub fn create_depth_buffer(&self) -> Option<Arc<vkimage::AttachmentImage<format::D16Unorm>>> {
    Some(vkimage::attachment::AttachmentImage::transient(
                                self.window.get_device().clone(),
                                self.window.get_dimensions(),                             
                                format::D16Unorm)
                                .unwrap())
  }
}

impl CoreRender for RawVk {  
  fn preload_model(&mut self, reference: String, location: String, texture: String) {
    self.load_model(reference.clone(), location, texture.clone());
    self.load_texture(reference, texture);
  }
  
  fn add_model(&mut self, reference: String, location: String, texture: String) {
    self.model_paths.insert(reference.clone(), Model_Info {location: location, texture: texture.clone()});
    self.add_texture(reference, texture);
  }
  
  fn load_model(&mut self, reference: String, location: String, texture: String) {
    let mut start_time = time::Instant::now();
    
    let model = model_data::Loader::load_opengex(location.clone(), texture);
    
    let vert3d_buffer = self.create_vertex(model.get_verticies().iter().cloned());
    let (idx_3d_buffer, future_3d_idx) = self.create_index(model.get_indicies().iter().cloned()); 
    
    let model = Model {
      vertex_buffer: vec!(vert3d_buffer),
      index_buffer: idx_3d_buffer,
    };
    self.models.insert(reference, model);
    
    self.previous_frame_end = Some(Box::new(future_3d_idx.join(Box::new(self.previous_frame_end.take().unwrap()) as Box<GpuFuture>)) as Box<GpuFuture>);
    
    let total_time = start_time.elapsed().subsec_nanos() as f64 / 1000000000.0 as f64;
    println!("{} ms,  {:?}", (total_time*1000f64) as f32, location);
  }
  
  fn pre_load_texture(&mut self, reference: String, location: String) {
    self.load_texture(reference, location);
  }
  
  fn add_texture(&mut self, reference: String, location: String) {
    self.texture_paths.insert(reference, location);
  }
  
  fn load_texture(&mut self, reference: String, location: String) {
    if location == String::from("") {
      return;
    }
    
    let texture_start_time = time::Instant::now();
    
    let (texture, tex_future) = {
      let image = image::open(&location).unwrap().to_rgba(); 
      let (width, height) = image.dimensions();
      let image_data = image.into_raw().clone();

      vkimage::immutable::ImmutableImage::from_iter(
              image_data.iter().cloned(),
              vkimage::Dimensions::Dim2d { width: width, height: height },
              format::R8G8B8A8Unorm,
               self.window.get_queue()).unwrap()
    };
    self.previous_frame_end = Some(Box::new(tex_future.join(Box::new(self.previous_frame_end.take().unwrap()) as Box<GpuFuture>)) as Box<GpuFuture>);
    self.textures.insert(reference.clone(), texture);
   
    let texture_time = texture_start_time.elapsed().subsec_nanos() as f64 / 1000000000.0 as f64;
    println!("{} ms,  {:?}", (texture_time*1000f64) as f32, location);
  }
  
  fn pre_load_font(&mut self, reference: String, font: &[u8], font_texture: String) {
    self.load_font(reference.clone(), font);    
    self.load_texture(reference, font_texture);
  }
  
  fn add_font(&mut self, reference: String, font: &[u8], font_texture: String) {
    self.load_font(reference.clone(), font);
    self.texture_paths.insert(reference, font_texture);
  }
  
  fn load_font(&mut self, reference: String, font: &[u8]) {
   let mut new_font = GenericFont::new();
    new_font.load_font(font);
    
    self.fonts.insert(reference.clone(), new_font);
  }
  
  fn load_shaders(&mut self) {
    let dimensions = {
      self.window.get_dimensions()
    };
    
    self.projection_2d = self.create_2d_projection(dimensions[0] as f32, dimensions[1] as f32);
    self.projection_3d = self.create_3d_projection(dimensions[0] as f32, dimensions[1] as f32);
    
    self.depth_buffer = self.create_depth_buffer();
    
    // 2D
    let vert_buffer = self.create_2d_vertex();
    let (idx_buffer, future_idx) = self.create_2d_index();
    
    self.vertex_buffer_2d = Some(vec!(vert_buffer));
    self.index_buffer_2d = Some(idx_buffer);
    
    self.previous_frame_end = Some(Box::new(future_idx.join(Box::new(self.previous_frame_end.take().unwrap()) as Box<GpuFuture>)) as Box<GpuFuture>);
    
    let vs_3d = vs_3d::Shader::load(self.window.get_device()).expect("failed to create shader module");
    let fs_3d = fs_3d::Shader::load(self.window.get_device()).expect("failed to create shader module");
    let vs_texture = vs_texture::Shader::load(self.window.get_device()).expect("failed to create shader module");
    let fs_texture = fs_texture::Shader::load(self.window.get_device()).expect("failed to create shader module");
    let vs_text = vs_text::Shader::load(self.window.get_device()).expect("failed to create shader module");
    let fs_text = fs_text::Shader::load(self.window.get_device()).expect("failed to create shader module");
    
    self.render_pass = Some(Arc::new(single_pass_renderpass!(self.window.get_device(),
      attachments: {
        colour: {
          load: Clear,
          store: Store,
          format: self.window.get_swapchain().format(),
          samples: 1,
        },
        depth: {
          load: Clear,
          store: DontCare,
          format: format::Format::D16Unorm,
          samples: 1,
        }
      },
      pass: {
        color: [colour],
        depth_stencil: {depth}
      }
    ).unwrap()));
   
    self.pipeline_3d = Some(Arc::new(pipeline::GraphicsPipeline::start()
        .vertex_input_single_buffer::<model_data::Vertex>()
        .vertex_shader(vs_3d.main_entry_point(), ())
        .triangle_list()
        .viewports_dynamic_scissors_irrelevant(1)
        .fragment_shader(fs_3d.main_entry_point(), ())
        .depth_stencil_simple_depth()
        .render_pass(framebuffer::Subpass::from(self.render_pass.clone().unwrap(), 0).unwrap())
        .build(self.window.get_device())
        .unwrap()));

    self.pipeline_texture = Some(Arc::new(pipeline::GraphicsPipeline::start()
        .vertex_input_single_buffer::<Vertex>()
        .vertex_shader(vs_texture.main_entry_point(), ())
        .triangle_strip()
        .viewports_dynamic_scissors_irrelevant(1)
        .fragment_shader(fs_texture.main_entry_point(), ())
        .blend_alpha_blending()
        .render_pass(framebuffer::Subpass::from(self.render_pass.clone().unwrap(), 0).unwrap())
        .build(self.window.get_device())
        .unwrap()));
        
    self.pipeline_text = Some(Arc::new(pipeline::GraphicsPipeline::start()
        .vertex_input_single_buffer::<Vertex>()
        .vertex_shader(vs_text.main_entry_point(), ())
        .triangle_strip()
        .viewports_dynamic_scissors_irrelevant(1)
        .fragment_shader(fs_text.main_entry_point(), ())
        .blend_alpha_blending()
        .render_pass(framebuffer::Subpass::from(self.render_pass.clone().unwrap(), 0).unwrap())
        .build(self.window.get_device())
        .unwrap()));
   
    self.uniform_buffer_texture = cpu_pool::CpuBufferPool::<vs_texture::ty::Data>::new(self.window.get_device(), BufferUsage::uniform_buffer());
    
    self.uniform_buffer_text = cpu_pool::CpuBufferPool::<vs_text::ty::Data>::new(self.window.get_device(), BufferUsage::uniform_buffer());
  }
  
  fn init(&mut self) {    
    self.framebuffers = None;
    
    self.recreate_swapchain = false;
  }
  
  fn dynamic_load(&mut self) {
    let time_limit = 9.0;
    
    let mut delta_time;
    let mut frame_start_time = time::Instant::now();
  
    let mut still_loading = false;
    let mut to_be_removed: Vec<String> = Vec::new();
    
    let texture_paths_clone = self.texture_paths.clone();
    
    for (reference, path) in &texture_paths_clone {
      self.load_texture(reference.clone(), path.clone());
      
      self.texture_paths.remove(reference);
      still_loading = true;
      
      delta_time = frame_start_time.elapsed().subsec_nanos() as f64 / 1000000000.0 as f64;
      if (delta_time*1000f64) > time_limit {
        break;
      } 
    }
    
    delta_time = frame_start_time.elapsed().subsec_nanos() as f64 / 1000000000.0 as f64;
    if (delta_time*1000f64) > time_limit {
      return;
    }
    
    let model_paths_clone = self.model_paths.clone();
    
    for (reference, model) in &model_paths_clone {
      self.load_model(reference.clone(), model.location.clone(), model.texture.clone());
      
      self.model_paths.remove(reference);
      still_loading = true;
      
      delta_time = frame_start_time.elapsed().subsec_nanos() as f64 / 1000000000.0 as f64;
      if (delta_time*1000f64) > time_limit {
        break;
      } 
    }
    
    if !still_loading {
      self.ready = true;
    }
  }
  
  fn clear_screen(&mut self) {
    self.previous_frame_end.as_mut().unwrap().cleanup_finished();
  }
  
  fn pre_draw(&mut self) {    
    if self.recreate_swapchain {
      let dimensions = {
        self.window.get_dimensions()
      };
      
      let (new_swapchain, new_images) = match self.window.recreate_swapchain(dimensions) {
        Ok(r) => r,
        Err(SwapchainCreationError::UnsupportedDimensions) => {
          return;
        },
        Err(err) => panic!("{:?}", err)
      };
      
      self.window.replace_swapchain(new_swapchain);
      self.window.replace_images(new_images);
      
      self.framebuffers = None;
      self.recreate_swapchain = false;
      
      let new_depth_buffer = self.create_depth_buffer();
      mem::replace(&mut self.depth_buffer, new_depth_buffer);
      
      self.projection_2d = self.create_2d_projection(dimensions[0] as f32, dimensions[1] as f32);
      self.projection_3d = self.create_3d_projection(dimensions[0] as f32, dimensions[1] as f32);
    }
    
    if self.framebuffers.is_none() {
      let depth_buffer = self.depth_buffer.clone();
      
      let new_framebuffers = 
        Some(self.window.get_images().iter().map( |image| {
             let fb = framebuffer::Framebuffer::start(self.render_pass.clone().unwrap())
                      .add(image.clone()).unwrap()
                      .add(depth_buffer.clone().unwrap()).unwrap()
                      .build().unwrap();
             Arc::new(fb) as Arc<framebuffer::FramebufferAbstract + Send + Sync>
             }).collect::<Vec<_>>());
      mem::replace(&mut self.framebuffers, new_framebuffers);
    }
  }
  
  fn draw(&mut self, draw_calls: &Vec<DrawCall>) {
   let (image_num, acquire_future) = match swapchain::acquire_next_image(self.window.get_swapchain(), None) {
      Ok(r) => r,
      Err(AcquireError::OutOfDate) => {
        self.recreate_swapchain = true;
        return;
      },
      Err(err) => panic!("{:?}", err)
    };
    
    let dimensions = {
      self.window.get_dimensions()
    };
    
    let command_buffer: AutoCommandBuffer = {
      let mut tmp_cmd_buffer = AutoCommandBufferBuilder::primary_one_time_submit(self.window.get_device(), self.window.get_queue_ref().family()).unwrap();
        
      let build_start = tmp_cmd_buffer;
        
      tmp_cmd_buffer = build_start.begin_render_pass(self.framebuffers.as_ref().unwrap()[image_num].clone(), false, vec![[0.2, 0.3, 0.3, 1.0].into(), 1f32.into()]).unwrap();    
      
      for draw in draw_calls {
        
        if draw.is_3d_model() {
          
          let uniform_buffer_subbuffer = {
            let rotation_x = cgmath::Matrix3::from_angle_x(cgmath::Rad(draw.get_rotation()));
            let rotation_y = cgmath::Matrix3::from_angle_y(cgmath::Rad(draw.get_y_rotation()));
            let rotation_z = cgmath::Matrix3::from_angle_z(cgmath::Rad(draw.get_z_rotation()));
                
            let world = cgmath::Matrix4::from_translation(draw.get_translation()) * cgmath::Matrix4::from(rotation_x) *  cgmath::Matrix4::from(rotation_y) * cgmath::Matrix4::from(rotation_z);
                
            let uniform_data = vs_3d::ty::Data {
              world: world.into(),
              view : (self.view * cgmath::Matrix4::from_scale(draw.get_size().x)).into(),
              proj : self.projection_3d.into(),
            };

            self.uniform_buffer_3d.next(uniform_data).unwrap()
          };
          
          let mut texture: String = String::from("default");
          if self.textures.contains_key(draw.get_texture()) {
            texture = draw.get_texture().clone();
          }
          
          let set_3d = Arc::new(descriptor_set::PersistentDescriptorSet::start(self.pipeline_3d.clone().unwrap(), 0)
                .add_buffer(uniform_buffer_subbuffer).unwrap()
                .add_sampled_image(self.textures.get(&texture).unwrap().clone(), self.sampler.clone()).unwrap()
                .build().unwrap()
          );
          
          {
            let mut cb = tmp_cmd_buffer;

            tmp_cmd_buffer = cb.draw_indexed(
                  self.pipeline_3d.clone().unwrap(),
                  DynamicState {
                        line_width: None,
                        viewports: Some(vec![pipeline::viewport::Viewport {
                            origin: [0.0, 0.0],
                            dimensions: [dimensions[0] as f32, dimensions[1] as f32],
                            depth_range: 0.0 .. 1.0,
                        }]),
                        scissors: None,
                  },
                  self.models.get(draw.get_texture()).expect("Invalid model name").vertex_buffer.clone(),
                  self.models.get(draw.get_texture()).expect("Invalid model name").index_buffer.clone(), set_3d.clone(), ()).unwrap();
          }
        } else {
          // Render Text
          if draw.get_text() != "" {
            let wrapped_draw = DrawMath::setup_correct_wrapping(draw.clone(), self.fonts.clone());
            let size = draw.get_x_size();
            
            for letter in wrapped_draw {              
              let char_letter = {
                letter.get_text().as_bytes()[0] 
              };
              
              let c = self.fonts.get(draw.get_texture()).unwrap().get_character(char_letter as i32);

              let model = DrawMath::calculate_text_model(letter.get_translation(), size, &c.clone(), char_letter);
              let letter_uv = DrawMath::calculate_text_uv(&c.clone());
              let colour = letter.get_colour();
              let outline = letter.get_outline_colour();
              let edge_width = letter.get_edge_width(); 
               
              let uniform_buffer_text_subbuffer = {
                let uniform_data = vs_text::ty::Data {
                  outlineColour: outline.into(),
                  colour: colour.into(),
                  edge_width: edge_width.into(),
                  letter_uv: letter_uv.into(),
                  model: model.into(),
                  projection: self.projection_2d.into(),
                };
                self.uniform_buffer_text.next(uniform_data).unwrap()
               };
              
              let uniform_set = Arc::new(descriptor_set::PersistentDescriptorSet::start(self.pipeline_text.clone().unwrap(), 0)
                                         .add_sampled_image(self.textures.get(draw.get_texture()).unwrap().clone(), self.sampler.clone()).unwrap()
                                         .add_buffer(uniform_buffer_text_subbuffer.clone()).unwrap()
                                         .build().unwrap());
              
              {
                let mut cb = tmp_cmd_buffer;
                tmp_cmd_buffer = cb.draw_indexed(self.pipeline_text.clone().unwrap(),
                                              DynamicState {
                                                      line_width: None,
                                                      viewports: Some(vec![Viewport {
                                                        origin: [0.0, 0.0],
                                                        dimensions: [dimensions[0] as f32, dimensions[1] as f32],
                                                        depth_range: 0.0 .. 1.0,
                                                      }]),
                                                      scissors: None,
                                              },
                                              self.vertex_buffer_2d.clone().unwrap(),
                                              self.index_buffer_2d.clone().unwrap(),
                                              uniform_set.clone(), ()).unwrap();
              
              
              }
            }
          } else {
            let model = DrawMath::calculate_texture_model(draw.get_translation(), draw.get_size());
          
            let uniform_buffer_subbuffer = {
              let uniform_data = vs_texture::ty::Data {
                colour: draw.get_colour().into(),
                model: model.into(),
                projection: self.projection_2d.into(),
              };
              self.uniform_buffer_texture.next(uniform_data).unwrap()
            };
            
            // No Texture
            if draw.get_texture() == &String::from("") {
              let uniform_set = Arc::new(descriptor_set::PersistentDescriptorSet::start(self.pipeline_texture.clone().unwrap(), 0)
                                         .add_sampled_image(self.textures.get("Candara").unwrap().clone(), self.sampler.clone()).unwrap()
                                         .add_buffer(uniform_buffer_subbuffer.clone()).unwrap()
                                         .build().unwrap());
              
              {
                let mut cb = tmp_cmd_buffer;
                
                tmp_cmd_buffer = cb.draw_indexed(self.pipeline_texture.clone().unwrap(),
                                              DynamicState {
                                                      line_width: None,
                                                      viewports: Some(vec![Viewport {
                                                        origin: [0.0, 0.0],
                                                        dimensions: [dimensions[0] as f32, dimensions[1] as f32],
                                                        depth_range: 0.0 .. 1.0,
                                                      }]),
                                                      scissors: None,
                                              },
                                              self.vertex_buffer_2d.clone().unwrap(),
                                              self.index_buffer_2d.clone().unwrap(),
                                              uniform_set.clone(), ()).unwrap();
              }
            } else {
              // Texture
              let uniform_set = Arc::new(descriptor_set::PersistentDescriptorSet::start(self.pipeline_texture.clone().unwrap(), 0)
                                      .add_sampled_image(self.textures.get(draw.get_texture()).expect("Unknown Texture").clone(), self.sampler.clone()).unwrap()
                                      .add_buffer(uniform_buffer_subbuffer.clone()).unwrap()
                                      .build().unwrap());
              
              {
                let mut cb = tmp_cmd_buffer;

                tmp_cmd_buffer = cb.draw_indexed(self.pipeline_texture.clone().unwrap(),
                                              DynamicState {
                                                      line_width: None,
                                                      viewports: Some(vec![Viewport {
                                                        origin: [0.0, 0.0],
                                                        dimensions: [dimensions[0] as f32, dimensions[1] as f32],
                                                        depth_range: 0.0 .. 1.0,
                                                      }]),
                                                      scissors: None,
                                              },
                                              self.vertex_buffer_2d.clone().unwrap(),
                                              self.index_buffer_2d.clone().unwrap(),
                                              uniform_set.clone(), ()).unwrap();
              }
            }
          }
        }
      }
        /*
        if draw.get_text() != "" {
          let wrapped_draw = DrawMath::setup_correct_wrapping(draw.clone(), self.fonts.clone());
          let size = draw.get_x_size();
          
          for letter in wrapped_draw {
            let cmd_tmp = cb;
            
            let char_letter = {
              letter.get_text().as_bytes()[0] 
            };
            
            let c = self.fonts.get(draw.get_texture()).unwrap().get_character(char_letter as i32);

            let model = DrawMath::calculate_text_model(letter.get_translation(), size, &c.clone(), char_letter);
            let letter_uv = DrawMath::calculate_text_uv(&c.clone());
            let colour = letter.get_colour();
            let outline = letter.get_outline_colour();
            let edge_width = letter.get_edge_width(); 
             
            let uniform_buffer_text_subbuffer = {
              let uniform_data = vs_text::ty::Data {
                outlineColour: outline.into(),
                colour: colour.into(),
                edge_width: edge_width.into(),
                letter_uv: letter_uv.into(),
                model: model.into(),
                projection: self.projection.into(),
              };
              self.uniform_buffer_text.next(uniform_data).unwrap()
             };
            
            let uniform_set = Arc::new(descriptor_set::PersistentDescriptorSet::start(self.pipeline_text.clone().unwrap(), 0)
                                       .add_sampled_image(self.textures.get(draw.get_texture()).unwrap().clone(), self.sampler.clone()).unwrap()
                                       .add_buffer(uniform_buffer_text_subbuffer.clone()).unwrap()
                                       .build().unwrap());
            
            cb = cmd_tmp.draw_indexed(self.pipeline_text.clone().unwrap(),
                                            DynamicState {
                                                    line_width: None,
                                                    viewports: Some(vec![Viewport {
                                                      origin: [0.0, 0.0],
                                                      dimensions: [dimensions[0] as f32, dimensions[1] as f32],
                                                      depth_range: 0.0 .. 1.0,
                                                    }]),
                                                    scissors: None,
                                            },
                                            self.vertex_buffer.clone().unwrap(),
                                            self.index_buffer.clone().unwrap(),
                                            uniform_set.clone(), ()).unwrap();
          }
          tmp_cmd_buffer = cb;
        } else {
          
          let model = DrawMath::calculate_texture_model(draw.get_translation(), draw.get_size());
          
          let uniform_buffer_subbuffer = {
            let uniform_data = vs_texture::ty::Data {
              colour: draw.get_colour().into(),
              model: model.into(),
              projection: self.projection.into(),
            };
            self.uniform_buffer_texture.next(uniform_data).unwrap()
          };
          
          // No Texture
          if draw.get_texture() == &String::from("") {
            let uniform_set = Arc::new(descriptor_set::PersistentDescriptorSet::start(self.pipeline_texture.clone().unwrap(), 0)
                                       .add_sampled_image(self.textures.get("Candara").unwrap().clone(), self.sampler.clone()).unwrap()
                                       .add_buffer(uniform_buffer_subbuffer.clone()).unwrap()
                                       .build().unwrap());
            
            tmp_cmd_buffer = cb.draw_indexed(self.pipeline_texture.clone().unwrap(),
                                            DynamicState {
                                                    line_width: None,
                                                    viewports: Some(vec![Viewport {
                                                      origin: [0.0, 0.0],
                                                      dimensions: [dimensions[0] as f32, dimensions[1] as f32],
                                                      depth_range: 0.0 .. 1.0,
                                                    }]),
                                                    scissors: None,
                                            },
                                            self.vertex_buffer.clone().unwrap(),
                                            self.index_buffer.clone().unwrap(),
                                            uniform_set.clone(), ()).unwrap();
          } else {
            // Texture
            let uniform_set = Arc::new(descriptor_set::PersistentDescriptorSet::start(self.pipeline_texture.clone().unwrap(), 0)
                                    .add_sampled_image(self.textures.get(draw.get_texture()).expect("Unknown Texture").clone(), self.sampler.clone()).unwrap()
                                    .add_buffer(uniform_buffer_subbuffer.clone()).unwrap()
                                    .build().unwrap());
          
            tmp_cmd_buffer = cb.draw_indexed(self.pipeline_texture.clone().unwrap(),
                                            DynamicState {
                                                    line_width: None,
                                                    viewports: Some(vec![Viewport {
                                                      origin: [0.0, 0.0],
                                                      dimensions: [dimensions[0] as f32, dimensions[1] as f32],
                                                      depth_range: 0.0 .. 1.0,
                                                    }]),
                                                    scissors: None,
                                            },
                                            self.vertex_buffer.clone().unwrap(),
                                            self.index_buffer.clone().unwrap(),
                                            uniform_set.clone(), ()).unwrap();
          }*/
       // }
      //}
      
      tmp_cmd_buffer.end_render_pass()
        .unwrap()
        .build().unwrap() as AutoCommandBuffer
    };
      
    let future = self.previous_frame_end.take().unwrap().join(acquire_future)
      .then_execute(self.window.get_queue(), command_buffer).unwrap()
      .then_swapchain_present(self.window.get_queue(), self.window.get_swapchain(), image_num)
      .then_signal_fence_and_flush().unwrap();
      
      
    self.previous_frame_end = Some(Box::new(future) as Box<_>);
  }
  
  fn screen_resized(&mut self) {
    self.recreate_swapchain = true;
  }
  
  fn get_dimensions(&self) -> [u32; 2] {
    let dimensions: [u32; 2] = self.window.get_dimensions();
    dimensions
  }
  
  fn get_events(&mut self) -> &mut winit::EventsLoop {
    self.window.get_events()
  }
  
  fn get_fonts(&self) -> HashMap<String, GenericFont> {
    self.fonts.clone()
  }
  
  fn get_dpi_scale(&self) -> f32 {
    self.window.get_dpi_scale()
  }
  
  fn is_ready(&self) -> bool {
    self.ready
  }
  
  fn show_cursor(&mut self) {
    self.window.show_cursor();
  }
  
  fn hide_cursor(&mut self) {
    self.window.hide_cursor();
  }
  
  fn set_camera_location(&mut self, camera: Vector3<f32>, camera_rot: Vector2<f32>) {

    let (x_rot, z_rot) = DrawMath::calculate_y_rotation(camera_rot.y);
    
    self.view = cgmath::Matrix4::look_at(cgmath::Point3::new(camera.x, camera.y, camera.z), cgmath::Point3::new(camera.x+x_rot, camera.y, camera.z+z_rot), cgmath::Vector3::new(0.0, -1.0, 0.0));  
  }
  
  fn post_draw(&self) {}
  fn clean(&self) {}
  fn swap_buffers(&mut self) {}
}


