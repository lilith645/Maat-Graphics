#![ allow( dead_code )]

pub extern crate ash;
pub extern crate winit;
pub extern crate image;

mod modules;
mod shader_handlers;

pub use crate::modules::{VkWindow};

use ash::vk;
use std::io::Cursor;
use std::time::Instant;

use crate::ash::version::DeviceV1_0;

use crate::modules::{Vulkan, Image, DescriptorSet, ComputeShader, DescriptorPoolBuilder};
use crate::shader_handlers::{TextureHandler, ModelHandler};
pub use crate::shader_handlers::{Camera, font::FontChar, Math};

use winit::{
  event::{Event, KeyboardInput, VirtualKeyCode, MouseButton, ElementState, WindowEvent, DeviceEvent},
  event_loop::{ControlFlow, EventLoop}
};

const DELTA_STEP: f32 = 0.001;
const ANIMATION_DELTA_STEP: f32 = 0.01;

pub enum MaatEvent<'a, T: Into<String>, L: Into<String>, S: Into<String>> {
  Draw(&'a mut Vec<(Vec<f32>, T, Option<L>)>, &'a mut Vec<(Vec<f32>, S)>),
  Update(&'a Vec<VirtualKeyCode>, &'a Vec<u32>, &'a mut Camera, f32),
  RealTimeInput(&'a Vec<VirtualKeyCode>, &'a mut Camera, f32),
  MouseMoved(f64, f64, &'a mut Camera),
  ScrollDelta(f32, &'a mut Camera),
  Resized(u32, u32),
  UnhandledWindowEvent(WindowEvent<'a>),
  UnhandledDeviceEvent(DeviceEvent)
}

pub struct MaatGraphics {
  vulkan: Vulkan,
  texture_handler: TextureHandler,
  model_handler: ModelHandler,
  compute_descriptor_pool: vk::DescriptorPool,
  compute_shader: ComputeShader,
  compute_descriptor_sets: DescriptorSet,
}

impl MaatGraphics {
  pub fn new(window: &mut VkWindow, screen_resolution: [u32; 2]) -> MaatGraphics {
    let screen_resolution = vk::Extent2D { width: screen_resolution[0], height: screen_resolution[1] };
    let mut vulkan = Vulkan::new(window, screen_resolution);
    
    let compute_descriptor_pool = DescriptorPoolBuilder::new()
                                              .num_storage(5)
                                              .build(vulkan.device());
    let compute_descriptor_sets = DescriptorSet::builder().storage_compute().build(vulkan.device(), &compute_descriptor_pool);
    let compute_shader = ComputeShader::new(vulkan.device(), 
                                            Cursor::new(&include_bytes!("../shaders/collatz_comp.spv")[..]),
                                            &compute_descriptor_sets);
    
    let mut compute_data = vec![64, 32, 8, 12, 96];
    vulkan.run_compute(&compute_shader, &compute_descriptor_sets, &mut compute_data);
    println!("Compute Data: {:?}", compute_data);
    
    let texture_handler = TextureHandler::new(&mut vulkan, screen_resolution);
    let model_handler = ModelHandler::new(&mut vulkan, screen_resolution);
    
    MaatGraphics {
      vulkan,
      texture_handler,
      model_handler,
      compute_descriptor_pool,
      compute_shader,
      compute_descriptor_sets,
    }
  }
  /*
  pub fn load_text(&mut self, text_ref: &str, text: &str, size: f32) {
    self.texture_handler.load_text(&mut self.vulkan, text_ref, text, size);
  }*/
  
  pub fn load_texture<T: Into<String>>(&mut self, texture_ref: T, texture: T) {
    self.texture_handler.load_texture(&mut self.vulkan, texture_ref, texture);
  }
  
  pub fn load_model<T: Into<String>>(&mut self, model_ref: T, model: T) {
    self.model_handler.load_model(&mut self.vulkan, model_ref, model);
  }
  
  pub fn all_model_bounding_boxes(&self) -> Vec<(String, Vec<([f32; 3], [f32; 3], [f32; 3])>)> {
    self.model_handler.all_model_bounding_boxes()
  }
  
  pub fn model_collision_meshes(&self) -> Vec<(String, Vec<[f32; 3]>, Vec<u32>)> {
    self.model_handler.model_collision_meshes()
  }
  
  pub fn get_font_data(&self) -> (Vec<FontChar>, u32, u32) {
    self.texture_handler.get_font_data()
  }
  
  pub fn recreate_swapchain(&mut self, width: u32, height: u32) {
    self.vulkan.swapchain().set_screen_resolution(
      width,
      height,
    );
    
    self.vulkan.recreate_swapchain();
    
    self.model_handler.mut_camera().update_aspect_ratio(width as f32/height as f32);
  }
  
  pub fn mut_camera(&mut self) -> &mut Camera {
    self.model_handler.mut_camera()
  }
  
  pub fn draw<T: Into<String>, L: Into<String>, S: Into<String>>(&mut self,
              texture_data: Vec<(Vec<f32>, T, Option<L>)>,
              model_data: Vec<(Vec<f32>, S)>
             ) {
    
    if self.model_handler.mut_camera().is_updated() {
      self.model_handler.update_uniform_buffer(self.vulkan.device());
    }
    
    if let Some(present_index) = self.vulkan.start_render() {
      self.vulkan.begin_renderpass_model(present_index);
      for (data, model) in model_data {
        self.model_handler.draw(&mut self.vulkan, data, &model.into());
      }
      self.vulkan.end_renderpass();
      self.vulkan.begin_renderpass_texture(present_index);
      for (data, texture, some_text) in texture_data {
        if let Some(text) = some_text {
          self.texture_handler.draw_text(&mut self.vulkan, data, &text.into(), &texture.into());
        } else {
          self.texture_handler.draw(&mut self.vulkan, data, &texture.into());
        }
      }
      self.vulkan.end_renderpass();
      self.vulkan.end_render(present_index);
    }
  }
  
  pub fn update_animations(&mut self, delta_time: f32) {
    self.model_handler.update_animations(&mut self.vulkan, delta_time);
  }
  
  pub fn destroy(&mut self) {
    unsafe {
      self.vulkan.device().internal().device_wait_idle().unwrap();
    }
    
    self.texture_handler.destroy(&mut self.vulkan);
    
    self.compute_descriptor_sets.destroy(self.vulkan.device());
    self.compute_shader.destroy(self.vulkan.device());
    
    unsafe {
      self.vulkan.device().destroy_descriptor_pool(self.compute_descriptor_pool, None);
    }
  }
  
  pub fn run<T, L, S, V>(mut vulkan: MaatGraphics, event_loop: EventLoop<()>, mut callback: T) -> !
         where
            T: 'static + FnMut(MaatEvent<L, S, V>),
            L: Into<String>,
            S: Into<String>,
            V: Into<String>, {
    let mut device_keys = Vec::new();
    let mut software_keys = Vec::new();
    
    let mut _delta_time = 0.0;
    let mut last_time = Instant::now();
    
    let mut total_delta_time = 0.0;
    let mut total_animation_delta_time = 0.0;
    
    event_loop.run(move |event, _, control_flow| {
      *control_flow = ControlFlow::Poll;
      
      _delta_time = last_time.elapsed().subsec_nanos() as f32 / 1000000000.0 as f32;
      last_time = Instant::now();
      total_delta_time += _delta_time as f32;
      total_animation_delta_time += _delta_time as f32;
      
      callback(MaatEvent::RealTimeInput(&device_keys, vulkan.mut_camera(), _delta_time));
      if total_delta_time > DELTA_STEP {
        let delta_steps = (total_delta_time / DELTA_STEP).floor() as usize;
        
        for _ in 0..delta_steps {
          callback(MaatEvent::Update(&device_keys, &software_keys, vulkan.mut_camera(), DELTA_STEP));
          total_delta_time -= DELTA_STEP;
        }
      }
      
      if total_animation_delta_time > ANIMATION_DELTA_STEP {
        let delta_steps = (total_animation_delta_time / ANIMATION_DELTA_STEP).floor() as usize;
        for _ in 0..delta_steps {
          vulkan.update_animations(ANIMATION_DELTA_STEP);
          total_animation_delta_time -= ANIMATION_DELTA_STEP;
        }
      }
      
      let mut texture_data = Vec::new();
      let mut model_data = Vec::new();
      
      callback(MaatEvent::Draw(&mut texture_data, &mut model_data));
      
      match event {
          Event::WindowEvent { event, .. } => match event {
            WindowEvent::CloseRequested => {
                *control_flow = ControlFlow::Exit;
            },
            WindowEvent::KeyboardInput {
                input:
                KeyboardInput {
                    virtual_keycode: Some(VirtualKeyCode::Escape),
                    ..
                },
                ..
            } => {
              *control_flow = ControlFlow::Exit
            },
            WindowEvent::Resized(dimensions) => {
              vulkan.recreate_swapchain(dimensions.width, dimensions.height);
              callback(MaatEvent::Resized(dimensions.width, dimensions.height));
            },
            WindowEvent::KeyboardInput {input, ..} => {
              let key_code = input.scancode;
              software_keys.push(key_code);
            },
            // TODO:
            WindowEvent::MouseInput {state, button, ..} => {
              match state {
                ElementState::Pressed => {
                  
                },
                ElementState::Released => {
                  
                },
              }
              
              match button {
                MouseButton::Left => {
                  
                },
                MouseButton::Right => {
                  
                },
                MouseButton::Middle => {
                  
                },
                MouseButton::Other(_id) => {
                  
                },
              }
            },
            window_event => {
              callback(MaatEvent::UnhandledWindowEvent(window_event));
             // handle_window_event(window_event, _delta_time);
            },
        },
        Event::DeviceEvent { event, .. } => match event {
          DeviceEvent::MouseMotion { delta: (mx, my) } => {
            callback(MaatEvent::MouseMoved(mx, my, vulkan.mut_camera()));
          },
          DeviceEvent::MouseWheel { delta } => {
            match delta {
              winit::event::MouseScrollDelta::LineDelta(_x, y) => {
                callback(MaatEvent::ScrollDelta(y, vulkan.mut_camera()));
              },
              _ => {},
            }
          },
          DeviceEvent::Key(key) => {
            match key.state {
              ElementState::Pressed => {
                if let Some(key_code) = key.virtual_keycode {
                  device_keys.push(key_code);
                }
              },
              ElementState::Released => {
                if let Some(key_code) = key.virtual_keycode {
                  let mut i = 0;
                  while i < device_keys.len() {
                    if device_keys[i] == key_code {
                      device_keys.remove(i);
                    }
                    
                    i += 1;
                  }
                }
              }
            }
          },
          device_event => {
            callback(MaatEvent::UnhandledDeviceEvent(device_event));
            //handle_device_event(device_event, &mut device_keys, vulkan.mut_camera(), _delta_time);
          }
        },
        Event::MainEventsCleared => {
          vulkan.draw(texture_data, model_data);
        },
        Event::LoopDestroyed => {
          vulkan.destroy();
        }
        _unhandled_event => {
          
        },
      }
    })
  }
}


#[cfg(test)]
mod tests {
  use super::*;
  
  #[test]
  fn length() {
    let vec3 = [1.0, 0.0, 0.0];
    let length = Math::vec3_mag(vec3);
    
    assert_eq!(length, 1.0);
  }
  
  #[test]
  fn dot_product() {
    let vec3_0 = [1.0, 0.0, 0.0];
    let vec3_1 = [1.0, 0.0, 0.0];
    
    let vec4_0 = [1.0, 0.0, 0.0, 0.0];
    let vec4_1 = [1.0, 0.0, 0.0, 0.0];
    
    let dot3 = Math::vec3_dot(vec3_0, vec3_1);
    let dot4 = Math::vec4_dot(vec4_0, vec4_1);
    
    assert_eq!(dot3, 1.0);
    assert_eq!(dot4, 1.0);
  }
  
  #[test]
  fn cross() {
    let cross_1 = Math::vec3_cross([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
    let cross_2 = Math::vec3_cross([0.0, 1.0, 0.0], [1.0, 0.0, 0.0]);
    
    assert_eq!(cross_1, [0.0, 0.0, 1.0]);
    assert_eq!(cross_2, [0.0, 0.0, -1.0]);
  }
  
  #[test]
  fn normalise() {
    let vec3_normalise_1 = Math::vec3_normalise([1.0, 0.0, 0.0]);
    let vec3_normalise_2 = Math::vec3_normalise([2.0, 0.0, 0.0]);
    
    let vec4_normalise_1 = Math::vec4_normalise([1.0, 0.0, 0.0, 0.0]);
    let vec4_normalise_2 = Math::vec4_normalise([2.0, 0.0, 0.0, 0.0]);
    
    assert_eq!(vec3_normalise_1, [1.0, 0.0, 0.0]);
    assert_eq!(vec3_normalise_2, [1.0, 0.0, 0.0]);
    
    assert_eq!(vec4_normalise_1, [1.0, 0.0, 0.0, 0.0]);
    assert_eq!(vec4_normalise_2, [1.0, 0.0, 0.0, 0.0]);
  }
  
  #[test]
  fn equals() {
    let vec3 = [1.0, 0.0, 0.0];
    let vec4 = [1.0, 0.0, 0.0, 0.0];
    
    assert_eq!(Math::vec3_equals(vec3, vec3), true);
    assert_eq!(Math::vec4_equals(vec4, vec4), true);
  }
  
  #[test]
  fn operators() {
    let a = [1.0, 2.0, 3.0];
    let b = [4.0, 5.0, 6.0];
    
    let c = Math::vec3_add(a, b);
    assert_eq!(c, [5.0, 7.0, 9.0]);
    
    let d = Math::vec3_minus(b, a);
    assert_eq!(d, [3.0, 3.0, 3.0]);
    
    let e = Math::vec3_mul(a, b);
    assert_eq!(e, [4.0, 10.0, 18.0]);
    
    let f = Math::vec3_div(b, a);
    assert_eq!(f, [4.0, 2.5, 2.0]);
    
    let g = Math::vec3_mul_f32(a, 2.0);
    assert_eq!(g, [2.0, 4.0, 6.0]);
    
    let h = Math::vec3_div_f32(b, 2.0);
    assert_eq!(h, [2.0, 2.5, 3.0]);
    
    let a = [1.0, 2.0, 3.0, 4.0];
    let b = [5.0, 6.0, 7.0, 8.0];
    
    let c = Math::vec4_add(a, b);
    assert_eq!(c, [6.0, 8.0, 10.0, 12.0]);
    
    let d = Math::vec4_minus(b, a);
    assert_eq!(d, [4.0, 4.0, 4.0, 4.0]);
    
    let e = Math::vec4_mul(a, b);
    assert_eq!(e, [5.0, 12.0, 21.0, 32.0]);
    
    let f = Math::vec4_div(b, a);
    assert_eq!(f, [5.0, 3.0, 7.0/3.0, 2.0]);
    
    let g = Math::vec4_mul_f32(a, 2.0);
    assert_eq!(g, [2.0, 4.0, 6.0, 8.0]);
    
    let h = Math::vec4_div_f32(b, 2.0);
    assert_eq!(h, [2.5, 3.0, 3.5, 4.0]);
  }
  
  #[test]
  fn mat4_multiply() {
    let m: [f32; 16] = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0];
    let n = Math::mat4_mul(m, m);
    /*let expected = [0.0, 1.0, 4.0, 9.0,
                    16.0, 25.0, 36.0, 49.0,
                    64.0, 81.0, 100.0, 121.0,
                    144.0, 169.0, 196.0, 255.0];*/
    let expected = [56.0, 62.0, 68.0, 74.0,
                    152.0, 174.0, 196.0, 218.0,
                    248.0, 286.0, 324.0, 362.0,
                    344.0, 398.0, 452.0, 506.0];
    
    assert_eq!(n, expected);
  }
  
  #[test]
  fn mat4_transpose() {
    let m: [f32; 16] = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0];
    let t = Math::mat4_transpose(m);
    let expected = [0.0, 4.0, 8.0, 12.0, 1.0, 5.0, 9.0, 13.0, 2.0, 6.0, 10.0, 14.0, 3.0, 7.0, 11.0, 15.0];
    
    assert_eq!(t, expected);
  }
  
  #[test]
  fn mat4_inverse() {
    let mut a = Math::mat4_identity();
    a[2] = 1.0;
    
    let b = Math::mat4_inverse(a);
    let i = Math::mat4_mul(a, b);
    
    assert_eq!(i, Math::mat4_identity());
  }
  
  #[test]
  fn mat4_scale() {
    let m = Math::mat4_identity();
    let v = [2.0, 2.0, 2.0];
    
    let s = Math::mat4_scale(m, v);
    let r = [2.0, 0.0, 0.0, 0.0,
             0.0, 2.0, 0.0, 0.0,
             0.0, 0.0, 2.0, 0.0,
             0.0, 0.0, 0.0, 1.0];
    
    assert_eq!(s, r);
  }
  
  #[test]
  fn mat4_det() {
    let m: [f32; 16] = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0];
    
    let det = Math::mat4_determinant(m);
    
    assert_eq!(det, 0.0);
  }
  
  /*
  #[test]
  fn mat4_rotate_eular() {
    let a = [1.0, 0.0, 0.0, 1.0];
    
    let r = Math::mat4_rotate_eular_axis(Math::mat4_identity(), (90.0f32).to_radians(), [0.0, 0.0, 1.0]);
    let b = Math::vec4_mul_mat4(a, r);
    
    assert_eq!(b, [0.0, 1.0, 0.0, 1.0]);
  }*/
  
  #[test]
  fn quat() {
    
  }
}






