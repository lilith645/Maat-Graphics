use vk;

use crate::math;
use crate::drawcalls;
use crate::font::GenericFont; 
use crate::OrthoCamera;

use crate::vulkan::vkenums::{ImageType, ImageUsage, ImageViewType, SampleCount, ImageTiling, AttachmentLoadOp, AttachmentStoreOp, ImageLayout, ImageAspect, ShaderStage, VertexInputRate};

use crate::vulkan::{Instance, Device, RenderPass, Shader, Pipeline, PipelineBuilder, DescriptorSet, UpdateDescriptorSets, DescriptorSetBuilder, ImageAttachment, AttachmentInfo, SubpassInfo, RenderPassBuilder, Sampler};
use crate::vulkan::buffer::{Buffer, BufferUsage, UniformBufferBuilder, UniformData, Framebuffer, CommandBufferBuilder};
use crate::vulkan::pool::{DescriptorPool, CommandPool};
use crate::CoreMaat;

use cgmath::{Vector2, Vector3, Vector4, Matrix4, SquareMatrix};

use std::mem;
use std::sync::Arc;
use std::collections::HashMap;

const MAX_INSTANCES: usize = 8096;

// Simple offset_of macro akin to C++ offsetof
#[macro_export]
macro_rules! offset_of {
    ($base:path, $field:ident) => {{
        #[allow(unused_unsafe)]
        unsafe {
            let b: $base = mem::uninitialized();
            (&b.$field as *const _ as isize) - (&b as *const _ as isize)
        }
    }};
}

#[derive(Clone)]
pub struct TextureInstanceData {
  model: Vector4<f32>,
  colour: Vector4<f32>,
  sprite_sheet: Vector4<f32>,
}

#[derive(Clone)]
pub struct Vertex {
  pos: Vector2<f32>,
  uvs: Vector2<f32>,
}

impl Vertex {
  pub fn vertex_input_binding() -> vk::VertexInputBindingDescription {
    vk::VertexInputBindingDescription {
      binding: 0,
      stride: (mem::size_of::<Vertex>()) as u32,
      inputRate: VertexInputRate::Vertex.to_bits(),
    }
  }
  
  pub fn vertex_input_attributes() -> Vec<vk::VertexInputAttributeDescription> {
    let mut vertex_input_attribute_descriptions: Vec<vk::VertexInputAttributeDescription> = Vec::with_capacity(2);
    
    vertex_input_attribute_descriptions.push(
      vk::VertexInputAttributeDescription {
        location: 0,
        binding: 0,
        format: vk::FORMAT_R32G32_SFLOAT,
        offset: offset_of!(Vertex, pos) as u32,
      }
    );
    
    vertex_input_attribute_descriptions.push(
      vk::VertexInputAttributeDescription {
        location: 1,
        binding: 0,
        format: vk::FORMAT_R32G32_SFLOAT,
        offset: offset_of!(Vertex, uvs) as u32,
      }
    );
    
    vertex_input_attribute_descriptions
  }
}

impl TextureInstanceData {
  pub fn vertex_input_binding() -> vk::VertexInputBindingDescription {
    vk::VertexInputBindingDescription {
      binding: 1,
      stride: (mem::size_of::<TextureInstanceData>()) as u32,
      inputRate: VertexInputRate::Instance.to_bits(),
    }
  }
  
  pub fn vertex_input_attributes() -> Vec<vk::VertexInputAttributeDescription> {
    let mut vertex_input_attribute_descriptions: Vec<vk::VertexInputAttributeDescription> = Vec::with_capacity(2);
    
    vertex_input_attribute_descriptions.push(
      vk::VertexInputAttributeDescription {
        location: 2,
        binding: 1,
        format: vk::FORMAT_R32G32B32A32_SFLOAT,
        offset: offset_of!(TextureInstanceData, model) as u32,
      }
    );
    
    vertex_input_attribute_descriptions.push(
      vk::VertexInputAttributeDescription {
        location: 3,
        binding: 1,
        format: vk::FORMAT_R32G32B32A32_SFLOAT,
        offset: offset_of!(TextureInstanceData, colour) as u32,
      }
    );
    
    vertex_input_attribute_descriptions.push(
      vk::VertexInputAttributeDescription {
        location: 4,
        binding: 1,
        format: vk::FORMAT_R32G32B32A32_SFLOAT,
        offset: offset_of!(TextureInstanceData, sprite_sheet) as u32,
      }
    );
    
    vertex_input_attribute_descriptions
  }
}

pub struct TextureShader {
  renderpass: RenderPass,
  framebuffers: Vec<Framebuffer>,
  framebuffer_colour_images: Vec<ImageAttachment>,
  framebuffer_msaa_images: Vec<ImageAttachment>,
  
  descriptor_sets: HashMap<String, DescriptorSet>,
  vertex_buffer: Buffer<Vertex>,
  index_buffer: Buffer<u32>,
  uniform_buffer: Buffer<f32>,
  
  texture_pipeline: Pipeline,
  text_pipeline: Pipeline,
  
  vertex_shader_texture: Shader,
  fragment_shader_texture: Shader,
  
  vertex_shader_text: Shader,
  fragment_shader_text: Shader,
  
  msaa: SampleCount,
  scale: f32,
  camera: OrthoCamera,
  
  vertex_shader_instanced: Shader,
  fragment_shader_instanced: Shader,
 // instanced_texture: String,
  instanced_cpu_buffers: HashMap<String, (UniformData, Buffer<f32>)>,
  //instanced_buffer: Buffer<f32>,
  instanced_descriptor_sets: HashMap<String, DescriptorSet>,
  instanced_pipeline: Pipeline,
}

impl TextureShader {
  pub fn new(instance: Arc<Instance>, device: Arc<Device>, current_extent: &vk::Extent2D, format: &vk::Format, sampler: &Sampler, image_views: &Vec<vk::ImageView>, texture_image: &ImageAttachment, descriptor_set_pool: &DescriptorPool, command_pool: &CommandPool, graphics_queue: &vk::Queue, msaa: &SampleCount) -> TextureShader {
    let vertex_shader_texture = Shader::new(Arc::clone(&device), include_bytes!("shaders/sprv/VkTextureVert.spv"));
    let fragment_shader_texture = Shader::new(Arc::clone(&device), include_bytes!("shaders/sprv/VkTextureFrag.spv"));
    let vertex_shader_text = Shader::new(Arc::clone(&device), include_bytes!("shaders/sprv/VkTextVert.spv"));
    let fragment_shader_text = Shader::new(Arc::clone(&device), include_bytes!("shaders/sprv/VkTextFrag.spv"));
    let vertex_shader_instanced = Shader::new(Arc::clone(&device), include_bytes!("shaders/sprv/VkTextureInstancedVert.spv"));
    let fragment_shader_instanced = Shader::new(Arc::clone(&device), include_bytes!("shaders/sprv/VkTextureInstancedFrag.spv"));
    
    let colour_attachment_load = if msaa != &SampleCount::OneBit { AttachmentLoadOp::DontCare } else { AttachmentLoadOp::Clear };
    
    let colour_attachment = AttachmentInfo::new()
                                .format(*format)
                                .multisample(&SampleCount::OneBit)
                                .load(colour_attachment_load)
                                .store(AttachmentStoreOp::Store)
                                .stencil_load(AttachmentLoadOp::DontCare)
                                .stencil_store(AttachmentStoreOp::DontCare)
                                .initial_layout(ImageLayout::Undefined)
                                .final_layout(ImageLayout::ColourAttachmentOptimal)
                                .image_usage(ImageLayout::ColourAttachmentOptimal);
    
    let msaa_attachment = AttachmentInfo::new()
                                .format(*format)
                                .multisample(msaa)
                                .load(AttachmentLoadOp::Clear)
                                .store(AttachmentStoreOp::Store)
                                .stencil_load(AttachmentLoadOp::DontCare)
                                .stencil_store(AttachmentStoreOp::DontCare)
                                .initial_layout(ImageLayout::Undefined)
                                .final_layout(ImageLayout::ColourAttachmentOptimal)
                                .image_usage(ImageLayout::ColourAttachmentOptimal);
    
    let mut subpass = SubpassInfo::new().add_colour_attachment(0);
    let mut render_pass = RenderPassBuilder::new();
    
    if msaa != &SampleCount::OneBit {
      subpass = subpass.add_resolve_attachment(1);
      render_pass = render_pass.add_attachment(msaa_attachment);
    }
    
    let render_pass = render_pass.add_attachment(colour_attachment)
                             .add_subpass(subpass)
                             .build(Arc::clone(&device));
    
    let (framebuffer_colour_images, 
         framebuffer_msaa_images, 
         framebuffers) = TextureShader::create_frame_buffers(Arc::clone(&instance), Arc::clone(&device), 
                                                             &render_pass, current_extent, format, msaa, 
                                                             image_views.len(), &command_pool,
                                                             graphics_queue);
    
    let mut descriptor_sets: HashMap<String, DescriptorSet> = HashMap::new();
    descriptor_sets.insert("".to_string(), DescriptorSetBuilder::new()
                           .fragment_combined_image_sampler(0)
                           .build(Arc::clone(&device), &descriptor_set_pool, 1));
    
    let mut instanced_descriptor_sets: HashMap<String, DescriptorSet> = HashMap::new();
    instanced_descriptor_sets.insert("".to_string(), DescriptorSetBuilder::new()
                           .fragment_combined_image_sampler(0)
                           .build(Arc::clone(&device), &descriptor_set_pool, 1));
    
    let push_constant_size = UniformData::new()
                               .add_matrix4(Matrix4::identity())
                               .add_vector4(Vector4::new(0.0, 0.0, 0.0, 0.0))
                               .add_vector4(Vector4::new(0.0, 0.0, 0.0, 0.0))
                               .add_vector4(Vector4::new(0.0, 0.0, 0.0, 0.0))
                               .size();
    
    let texture_pipeline = PipelineBuilder::new()
                  .vertex_shader(*vertex_shader_texture.get_shader())
                  .fragment_shader(*fragment_shader_texture.get_shader())
                  .push_constants(ShaderStage::Vertex, push_constant_size as u32)
                  .render_pass(render_pass.clone())
                  .descriptor_set_layout(descriptor_sets.get(&"".to_string()).unwrap().layouts_clone())
                  .vertex_binding(vec!(Vertex::vertex_input_binding()))
                  .vertex_attributes(Vertex::vertex_input_attributes())
                  .multisample(msaa)
                  .topology_triangle_list()
                  .polygon_mode_fill()
                  .cull_mode_back()
                  .front_face_counter_clockwise()
                  .build(Arc::clone(&device));
    
    let text_pipeline = TextureShader::create_text_pipline(Arc::clone(&device), &vertex_shader_text, &fragment_shader_text, &render_pass, msaa, &descriptor_sets);
    
    let push_constant_size = UniformData::new()
                               .add_vector4(Vector4::new(0.0, 0.0, 0.0, 0.0))
                               .size();
    
    let mut attributes: Vec<vk::VertexInputAttributeDescription> = Vertex::vertex_input_attributes();
    attributes.append(&mut TextureInstanceData::vertex_input_attributes());
    
    let instanced_pipeline = PipelineBuilder::new()
                  .vertex_shader(*vertex_shader_instanced.get_shader())
                  .fragment_shader(*fragment_shader_instanced.get_shader())
                  .push_constants(ShaderStage::Vertex, push_constant_size as u32)
                  .render_pass(render_pass.clone())
                  .descriptor_set_layout(instanced_descriptor_sets.get(&"".to_string()).unwrap().layouts_clone())
                  .vertex_binding(vec!(Vertex::vertex_input_binding(), TextureInstanceData::vertex_input_binding()))
                  .multisample(msaa)
                  .vertex_attributes(attributes)
                  .topology_triangle_list()
                  .polygon_mode_fill()
                  .cull_mode_back()
                  .front_face_counter_clockwise()
                  .build(Arc::clone(&device));
    
    let vertex_buffer = TextureShader::create_vertex_buffer(Arc::clone(&instance), Arc::clone(&device), &command_pool, graphics_queue);
    let index_buffer = TextureShader::create_index_buffer(Arc::clone(&instance), Arc::clone(&device), &command_pool, graphics_queue);
    
    let uniform_buffer_description = UniformBufferBuilder::new().add_matrix4();
    
    let uniform_buffer = TextureShader::create_uniform_buffer(Arc::clone(&instance), Arc::clone(&device), image_views.len() as u32, uniform_buffer_description);
    
    let camera = OrthoCamera::new(current_extent.width as f32, current_extent.height as f32);
    TextureShader::update_uniform_buffers(Arc::clone(&device), &texture_image, sampler, descriptor_sets.get("").unwrap());
    
    TextureShader {
      renderpass: render_pass,
      framebuffers,
      framebuffer_colour_images,
      framebuffer_msaa_images,
      
      descriptor_sets,
      vertex_buffer,
      index_buffer,
      uniform_buffer,
      
      texture_pipeline,
      text_pipeline,
      
      vertex_shader_texture,
      fragment_shader_texture,
      
      vertex_shader_text,
      fragment_shader_text,
      
      msaa: *msaa,
      scale: 1.0,
      camera,
      
      vertex_shader_instanced,
      fragment_shader_instanced,
      instanced_cpu_buffers: HashMap::new(),
      instanced_descriptor_sets,
      instanced_pipeline,
    }
  }
  
  pub fn set_scale(&mut self, new_scale: f32) {
    self.scale = new_scale;
  }
  
  pub fn lerp_camera(&mut self, position: Vector2<f32>, vel: Vector2<f32>) {
    self.camera.lerp_to_position(position, vel);
  }
  
  pub fn lerp_camera_to_size(&mut self, size: Vector2<f32>, vel: Vector2<f32>) {
    self.camera.lerp_to_size(size, vel);
  }
  
  pub fn reset_camera(&mut self, width: f32, height: f32) {
    self.camera.reset(width, height);
  }
  
  pub fn get_texture(&mut self, current_buffer: usize) -> ImageAttachment {
    self.framebuffer_colour_images[current_buffer].clone()
  }
  
  pub fn recreate(&mut self, instance: Arc<Instance>, device: Arc<Device>, format: &vk::Format, image_views: &Vec<vk::ImageView>, new_extent: &vk::Extent2D, textures: Vec<(String, ImageAttachment)>, sampler: &Sampler, command_pool: &CommandPool, graphics_queue: &vk::Queue) {
    for i in 0..self.framebuffers.len() {
      self.framebuffers[i].destroy(Arc::clone(&device));
      self.framebuffer_colour_images[i].destroy(Arc::clone(&device));
      
      if self.msaa != SampleCount::OneBit {
        self.framebuffer_msaa_images[i].destroy(Arc::clone(&device));
      }
    }
    
    self.framebuffers.clear();
    self.framebuffer_colour_images.clear();
    self.framebuffer_msaa_images.clear();
    
    let (framebuffer_colour_images, 
         framebuffer_msaa_images, 
         framebuffers) = TextureShader::create_frame_buffers(Arc::clone(&instance), Arc::clone(&device), 
                                                             &self.renderpass, new_extent, format, 
                                                             &self.msaa, image_views.len(), command_pool, 
                                                             graphics_queue);
    
    self.framebuffers = framebuffers;
    self.framebuffer_colour_images = framebuffer_colour_images;
    self.framebuffer_msaa_images = framebuffer_msaa_images;
    
    self.camera.window_resized(new_extent.width as f32, new_extent.height as f32);
    
    self.update_uniform(device, textures, sampler);
  }
  
  pub fn update_uniform(&mut self, device: Arc<Device>, textures: Vec<(String, ImageAttachment)>, sampler: &Sampler) {
    for (key, descriptor) in &self.descriptor_sets {
      for i in 0..textures.len() {
        if key.to_string() == textures[i].0 {
          TextureShader::update_uniform_buffers(Arc::clone(&device), &textures[i].1, sampler, descriptor);
        }
      }
    }
    
    for (key, descriptor) in &self.instanced_descriptor_sets {
      for i in 0..textures.len() {
        if key.to_string() == textures[i].0 {
          UpdateDescriptorSets::new()
             .add_sampled_image(0, &textures[i].1, ImageLayout::ShaderReadOnlyOptimal, &sampler)
             .finish_update(Arc::clone(&device), &descriptor);
        }
      }
    }
  }
  
  pub fn add_instanced_buffer(&mut self, instance: Arc<Instance>, device: Arc<Device>, image_views: u32, reference: String) {
    //  let usage = BufferUsage::vertex_transfer_dst_buffer();
    //  let instanced_buffer = Buffer::device_local_buffer(Arc::clone(&instance), Arc::clone(&device), usage, image_views.len() as u32, instanced_data.clone());
    let mut instanced_data = Vec::with_capacity(MAX_INSTANCES*12);
    for _ in 0..(MAX_INSTANCES*12) {
      instanced_data.push(0.0);
    }
    
    let usage = BufferUsage::vertex_transfer_src_buffer();
    let instanced_cpu_buffer = Buffer::cpu_buffer(Arc::clone(&instance), Arc::clone(&device), usage, image_views, instanced_data);
    self.instanced_cpu_buffers.insert(reference, (UniformData::with_capacity(MAX_INSTANCES*12), instanced_cpu_buffer));
  }
  
  pub fn add_texture(&mut self, device: Arc<Device>, descriptor_set_pool: &DescriptorPool, texture_reference: String, texture_image: &ImageAttachment, sampler: &Sampler) {
   if !self.descriptor_sets.contains_key(&texture_reference) {
      let descriptor = DescriptorSetBuilder::new()
                           .fragment_combined_image_sampler(0)
                           .build(Arc::clone(&device), &descriptor_set_pool, 1);
      self.descriptor_sets.insert(texture_reference.to_string(), descriptor);
      
      if let Some(descriptor_set) = self.descriptor_sets.get(&texture_reference) {
        TextureShader::update_uniform_buffers(Arc::clone(&device), &texture_image, sampler, &descriptor_set);
      }
    }
    
    if !self.instanced_descriptor_sets.contains_key(&texture_reference) {
      let descriptor = DescriptorSetBuilder::new()
                           .fragment_combined_image_sampler(0)
                           .build(Arc::clone(&device), &descriptor_set_pool, 1);
      self.instanced_descriptor_sets.insert(texture_reference.to_string(), descriptor);
      
      if let Some(descriptor_set) = self.instanced_descriptor_sets.get(&texture_reference) {
         UpdateDescriptorSets::new()
             .add_sampled_image(0, texture_image, ImageLayout::ShaderReadOnlyOptimal, &sampler)
             .finish_update(Arc::clone(&device), &descriptor_set);
      }
    }
  }
  
  fn create_text_pipline(device: Arc<Device>, vertex_shader: &Shader, fragment_shader: &Shader, render_pass: &RenderPass, msaa: &SampleCount, descriptor_sets: &HashMap<String, DescriptorSet>) -> Pipeline {
    let push_constant_size = UniformData::new()
                               .add_vector4(Vector4::new(0.0, 0.0, 0.0, 0.0))
                               .add_vector4(Vector4::new(0.0, 0.0, 0.0, 0.0))
                               .add_vector4(Vector4::new(0.0, 0.0, 0.0, 0.0))
                               .add_vector4(Vector4::new(0.0, 0.0, 0.0, 0.0))
                               .add_vector4(Vector4::new(0.0, 0.0, 0.0, 0.0))
                               .size();
    
    PipelineBuilder::new()
                  .vertex_shader(*vertex_shader.get_shader())
                  .fragment_shader(*fragment_shader.get_shader())
                  .push_constants(ShaderStage::Vertex, push_constant_size as u32)
                  .render_pass(render_pass.clone())
                  .descriptor_set_layout(descriptor_sets.get(&"".to_string()).unwrap().layouts_clone())
                  .vertex_binding(vec!(Vertex::vertex_input_binding()))
                  .vertex_attributes(Vertex::vertex_input_attributes())
                  .multisample(msaa)
                  .topology_triangle_list()
                  .polygon_mode_fill()
                  .cull_mode_back()
                  .front_face_clockwise()
                  .build(Arc::clone(&device))
  }
  
  fn create_uniform_buffer(instance: Arc<Instance>, device: Arc<Device>, num_sets: u32, uniform_buffer: UniformBufferBuilder) -> Buffer<f32> {
    let buffer = uniform_buffer.build(Arc::clone(&instance), Arc::clone(&device), num_sets);
    
    buffer
  }
  
  pub fn create_index_buffer(instance: Arc<Instance>, device: Arc<Device>, command_pool: &CommandPool, graphics_queue: &vk::Queue) -> Buffer<u32> {
    let indices = vec!(0, 1, 2, 2, 3, 0);
    
    let usage_src = BufferUsage::index_transfer_src_buffer();
    let usage_dst = BufferUsage::index_transfer_dst_buffer();
    
    let staging_buffer: Buffer<u32> = Buffer::cpu_buffer(Arc::clone(&instance), Arc::clone(&device), usage_src, 1, indices.clone());
    let buffer: Buffer<u32> = Buffer::device_local_buffer(Arc::clone(&instance), Arc::clone(&device), usage_dst, 1, indices);
    
    let command_buffer = CoreMaat::begin_single_time_command(Arc::clone(&device), &command_pool);
    command_buffer.copy_buffer(Arc::clone(&device), &staging_buffer, &buffer, 0);
    CoreMaat::end_single_time_command(Arc::clone(&device), command_buffer, &command_pool, graphics_queue);
    
    staging_buffer.destroy(Arc::clone(&device));
    
    buffer
  }
  
  pub fn create_vertex_buffer(instance: Arc<Instance>, device: Arc<Device>, command_pool: &CommandPool, graphics_queue: &vk::Queue) -> Buffer<Vertex> {
    let triangle = vec!(
      Vertex { pos: Vector2::new(0.5, 0.5), uvs: Vector2::new(0.99, 0.0) },
      Vertex { pos: Vector2::new(-0.5, 0.5), uvs: Vector2::new(0.0, 0.0) },
      Vertex { pos: Vector2::new(-0.5, -0.5), uvs: Vector2::new(0.0, 0.99) },
      Vertex { pos: Vector2::new(0.5, -0.5), uvs: Vector2::new(0.99, 0.99) },
    );
    
    let usage_src = BufferUsage::vertex_transfer_src_buffer();
    let usage_dst = BufferUsage::vertex_transfer_dst_buffer();
    
    let staging_buffer: Buffer<Vertex> = Buffer::cpu_buffer(Arc::clone(&instance), Arc::clone(&device), usage_src, 1, triangle.clone());
    let buffer: Buffer<Vertex> = Buffer::device_local_buffer(Arc::clone(&instance), Arc::clone(&device), usage_dst, 1, triangle);
    
    let command_buffer = CoreMaat::begin_single_time_command(Arc::clone(&device), &command_pool);
    command_buffer.copy_buffer(Arc::clone(&device), &staging_buffer, &buffer, 0);
    CoreMaat::end_single_time_command(Arc::clone(&device), command_buffer, &command_pool, graphics_queue);
    
    staging_buffer.destroy(Arc::clone(&device));
    
    buffer
  }
  
  fn create_frame_buffers(instance: Arc<Instance>, device: Arc<Device>, render_pass: &RenderPass, swapchain_extent: &vk::Extent2D, format: &vk::Format, msaa: &SampleCount, num_image_views: usize, command_pool: &CommandPool, graphics_queue: &vk::Queue) -> (Vec<ImageAttachment>, Vec<ImageAttachment>, Vec<Framebuffer>) {
    
    let mut framebuffer_colour_images = Vec::with_capacity(num_image_views);
    let mut framebuffer_msaa_images = Vec::with_capacity(num_image_views);
    
    for _ in 0..num_image_views {
      framebuffer_colour_images.push(ImageAttachment::create_image_colour_attachment(Arc::clone(&instance), Arc::clone(&device), &ImageType::Type2D, &ImageTiling::Optimal, &ImageUsage::transfer_src_colour_attachment_sampled(), &ImageLayout::Undefined, &SampleCount::OneBit, &ImageViewType::Type2D, format, swapchain_extent.width as u32, swapchain_extent.height as u32));
      
      if msaa != &SampleCount::OneBit {
        framebuffer_msaa_images.push(ImageAttachment::create_image_msaa_attachment(Arc::clone(&instance), Arc::clone(&device), &ImageType::Type2D, &ImageTiling::Optimal, &ImageUsage::transient_colour_attachment(), &ImageLayout::Undefined, &ImageLayout::ColourAttachmentOptimal, &ImageAspect::Colour, msaa, &ImageViewType::Type2D, format, command_pool, graphics_queue, swapchain_extent.width as u32, swapchain_extent.height as u32));
      }
    }
    
    let mut framebuffers: Vec<Framebuffer> = Vec::with_capacity(framebuffer_colour_images.len());
    
    for i in 0..framebuffer_colour_images.len() {
      let mut temp_image_views = Vec::new();
      
      if msaa != &SampleCount::OneBit {
        temp_image_views.push(framebuffer_msaa_images[i].get_image_view().clone());
      }
      
      temp_image_views.push(framebuffer_colour_images[i].get_image_view().clone());
      
      let framebuffer: Framebuffer = Framebuffer::new_with_imageviews(Arc::clone(&device), render_pass, swapchain_extent, temp_image_views);
      
      framebuffers.push(framebuffer)
    }
    
    (framebuffer_colour_images, framebuffer_msaa_images, framebuffers)
  }
  
  fn update_uniform_buffers(device: Arc<Device>, texture: &ImageAttachment, sampler: &Sampler, descriptor_sets: &DescriptorSet) {
    UpdateDescriptorSets::new()
       .add_sampled_image(0, texture, ImageLayout::ShaderReadOnlyOptimal, &sampler)
       .finish_update(Arc::clone(&device), &descriptor_sets);
  }
  
  pub fn begin_renderpass(&mut self, device: Arc<Device>, cmd: CommandBufferBuilder, clear_value: &Vec<vk::ClearValue>, window_size: &vk::Extent2D, current_buffer: usize) -> CommandBufferBuilder {
    cmd.begin_render_pass(Arc::clone(&device), &clear_value, &self.renderpass, &self.framebuffers[current_buffer].internal_object(), &window_size)
  }
  
  pub fn draw_texture(&mut self, device: Arc<Device>, cmd: CommandBufferBuilder, position: Vector2<f32>, scale: Vector2<f32>, rotation: f32, sprite_details: Option<Vector3<i32>>, colour: Option<Vector4<f32>>, use_texture: bool, texture_reference: String) -> CommandBufferBuilder {
    let mut cmd = cmd;
    
    if !self.descriptor_sets.contains_key(&texture_reference) {
      return cmd
    }
    
    let descriptor: &DescriptorSet = self.descriptor_sets.get(&texture_reference).unwrap();
    
    let model = math::calculate_texture_model(Vector3::new(position.x, position.y, 0.0), scale, -rotation -180.0);
    
    
    let mut sprite = {
      let mut tex_view = Vector4::new(0.0, 0.0, 1.0, self.scale);
      if let Some(details) = sprite_details {
        tex_view = Vector4::new(details.x as f32, details.y as f32, details.z as f32, self.scale);
      }
      tex_view
    };
    
    if use_texture {
      sprite.z *= -1.0;
    }
    
    let draw_colour;
    if let Some(colour) = colour {
      draw_colour = colour;
    } else {
      draw_colour = Vector4::new(1.0, 1.0, 1.0, 1.0);
    }
    
    let top = self.camera.get_top();
    let right = self.camera.get_right();
    let pos = self.camera.get_position();
    let projection_details = Vector4::new(pos.x, pos.y, right, top);
    
    let push_constant_data = UniformData::new()
                               .add_matrix4(model)
                               .add_vector4(draw_colour)
                               .add_vector4(sprite)
                               .add_vector4(projection_details);
    
    cmd = cmd.push_constants(Arc::clone(&device), &self.texture_pipeline, ShaderStage::Vertex, push_constant_data);
    
    let index_count = 6;
    
    cmd.draw_indexed(Arc::clone(&device), &self.vertex_buffer.internal_object(0),
                             &self.index_buffer.internal_object(0),
                             index_count, &self.texture_pipeline,
                             vec!(*descriptor.set(0)))
  }
  
  pub fn draw_text(&mut self, device: Arc<Device>, cmd: CommandBufferBuilder, display_text: String, font: String, position: Vector2<f32>, scale: Vector2<f32>, colour: Vector4<f32>, outline_colour: Vector3<f32>, edge_width: Vector4<f32>, wrap_length: u32, centered: bool, font_details: GenericFont, window_width: f32, window_height: f32) -> CommandBufferBuilder {
    let mut cmd = cmd;
    
    if !self.descriptor_sets.contains_key(&font) {
      return cmd
    }
    
    let descriptor: &DescriptorSet = self.descriptor_sets.get(&font).unwrap();
    
    
    let wrapped_draw = drawcalls::setup_correct_wrapping(display_text.clone(), font, position, scale*2.0, colour, outline_colour, edge_width, wrap_length, centered, font_details.clone());
    
    let scale = scale.x;
    for letter in wrapped_draw {
      let (_font, display_text, position, _scale, colour, outline_colour, edge_width, _wrapped, _wrap_length, _centered) = letter.draw_font_details().unwrap();
      let char_letter = {
        display_text.as_bytes()[0] 
      };
      
      let c = font_details.get_character(char_letter as i32);
      
      let mut model = drawcalls::calculate_text_info(Vector3::new(position.x, position.y, 0.0), scale, &c.clone(), char_letter);
      model.z *= scale/(scale/2.0);
      model.w = window_width;
      let letter_uv = drawcalls::calculate_text_uv(&c.clone());
      let colour = colour;
      let outline = Vector4::new(outline_colour.x, outline_colour.y, outline_colour.z, window_height);
      let edge_width = edge_width; 
      
      let push_constant_data = UniformData::new()
                                .add_vector4(model)
                                .add_vector4(letter_uv)
                                .add_vector4(edge_width)
                                .add_vector4(colour)
                                .add_vector4(outline);
      
      cmd = cmd.push_constants(Arc::clone(&device), &self.text_pipeline, ShaderStage::Vertex, push_constant_data);
      
      let index_count = 6;
      
      cmd = cmd.draw_indexed(Arc::clone(&device), &self.vertex_buffer.internal_object(0),
                               &self.index_buffer.internal_object(0),
                               index_count, &self.text_pipeline,
                               vec!(*descriptor.set(0)))
    }
    
    cmd
  }
  
  pub fn add_instanced_draw(&mut self, position: Vector2<f32>, scale: Vector2<f32>, rotation: f32, sprite_details: Option<Vector3<i32>>, colour: Vector4<f32>, use_texture: bool, buffer_reference: String) {
    let model = Vector4::new(position.x, position.y, scale.x, -rotation-180.0);
    
    let mut sprite = {
      let mut tex_view = Vector4::new(0.0, 0.0, 1.0, self.scale);
      if let Some(details) = sprite_details {
        tex_view = Vector4::new(details.x as f32, details.y as f32, details.z as f32, self.scale);
      }
      tex_view
    };
    
    if use_texture {
      sprite.z *= -1.0;
    }
    
    let draw_colour = colour;
    let mut details = self.instanced_cpu_buffers.get_mut(&buffer_reference).unwrap();
    
    let data = details.0.clone();
    details.0 = data
                      .add_vector4(model)
                      .add_vector4(draw_colour)
                      .add_vector4(sprite);
  }
  
  pub fn draw_instanced(&mut self, device: Arc<Device>, cmd: CommandBufferBuilder, current_buffer: usize, buffer_reference: String, texture_reference: String) -> CommandBufferBuilder {
    let mut cmd = cmd;
    
    if let Some((instanced_data, buffer)) = self.instanced_cpu_buffers.get_mut(&buffer_reference) {
      let data = instanced_data.build();
      let num_instances = data.len() as u32 / 12;
      
      if num_instances == 0 {
        return cmd;
      }
      
      buffer.fill_buffer(Arc::clone(&device), current_buffer, data);
      
      let descriptor: &DescriptorSet = self.instanced_descriptor_sets.get(&texture_reference).unwrap();
      
      let top = self.camera.get_top();
      let right = self.camera.get_right();
      let pos = self.camera.get_position();
      let projection = Vector4::new(pos.x, pos.y, right, top);
      
      let push_constant_data = UniformData::new()
                                .add_vector4(projection);
      
      cmd = cmd.push_constants(Arc::clone(&device), &self.instanced_pipeline, ShaderStage::Vertex, push_constant_data);
      
      let index_count = 6;
      
      cmd = cmd.draw_instanced_indexed(Arc::clone(&device), 
                                       &self.vertex_buffer.internal_object(0),
                                       &self.index_buffer.internal_object(0),
                                       &buffer.internal_object(current_buffer),
                                       index_count,
                                       num_instances,
                                       &self.instanced_pipeline,
                                       vec!(*descriptor.set(0)));
      
      instanced_data.empty();
    }
    
    cmd
  }
  
  pub fn fill_buffers(&mut self, _instance: Arc<Instance>, _device: Arc<Device>, cmd: CommandBufferBuilder, _current_buffer: usize) -> CommandBufferBuilder {
    /*
    let mut cmd = cmd;
    
    let data = self.instanced_data.build();
    let num_instances = data.len() as u32;
    
    if num_instances == 0 {
      return cmd;
    }
    
    let usage = BufferUsage::transfer_src_buffer();
    let mut cpu_buffer = Buffer::cpu_buffer(Arc::clone(&instance), Arc::clone(&device), usage, 1, data.clone());
    
    cpu_buffer.fill_buffer(Arc::clone(&device), data);
    cmd = cmd.copy_buffer_to_buffer(Arc::clone(&device), &cpu_buffer, &self.instanced_buffer, 0);
    
    self.instanced_data.empty();
    */
    cmd
  }
  
  pub fn destroy(&mut self, device: Arc<Device>) {
    self.uniform_buffer.destroy(Arc::clone(&device));
    
    self.index_buffer.destroy(Arc::clone(&device));
    self.vertex_buffer.destroy(Arc::clone(&device));
   // self.instanced_buffer.destroy(Arc::clone(&device));
    for instance_details in self.instanced_cpu_buffers.iter() {
      match instance_details {
        (_reference, (_data, buffer)) => {
          buffer.destroy(Arc::clone(&device));
        }
      }
    }
    
    self.texture_pipeline.destroy(Arc::clone(&device));
    self.text_pipeline.destroy(Arc::clone(&device));
    self.instanced_pipeline.destroy(Arc::clone(&device));
    
    for (_reference, descriptor_set) in &self.descriptor_sets {
      descriptor_set.destroy(Arc::clone(&device));
    }
    
    for (_reference, descriptor_set) in &self.instanced_descriptor_sets {
      descriptor_set.destroy(Arc::clone(&device));
    }
    
    self.vertex_shader_texture.destroy(Arc::clone(&device));
    self.fragment_shader_texture.destroy(Arc::clone(&device));
    self.vertex_shader_text.destroy(Arc::clone(&device));
    self.fragment_shader_text.destroy(Arc::clone(&device));
    self.vertex_shader_instanced.destroy(Arc::clone(&device));
    self.fragment_shader_instanced.destroy(Arc::clone(&device));
    
    for framebuffer in &self.framebuffers {
     framebuffer.destroy(Arc::clone(&device));
    }
    
    for images in &self.framebuffer_colour_images {
      images.destroy(Arc::clone(&device));
    }
    for images in &self.framebuffer_msaa_images {
      images.destroy(Arc::clone(&device));
    }
    
    self.renderpass.destroy(Arc::clone(&device));
  }
}
