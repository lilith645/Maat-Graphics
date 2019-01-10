use crate::ThreadPool;

use vk;
use image;

use crate::vulkan::vkenums::{ImageType, ImageViewType, ImageTiling, Sample};

use crate::vulkan::{Image, Instance, Device};
use crate::vulkan::buffer::{Buffer};
use crate::vulkan::pool::{CommandPool};

use crate::font::GenericFont;
use crate::graphics::Vertex2d;

use std::time;
use std::sync::Arc;
use std::sync::mpsc;
use std::sync::Mutex;

#[derive(Clone)]
enum ObjectType {
  Font(Option<(GenericFont, Image)>),
  Texture(Option<Image>),
  _Model(String),
  Shape(Option<(Buffer<f32>, Image)>),
}

#[derive(Clone)]
struct LoadableObject {
  pub loaded: bool,
  pub location: String,
  pub reference: String,
  pub object_type: ObjectType,
}

pub struct ResourceManager {
  objects: Vec<LoadableObject>,
  pool: ThreadPool,
  num_recv_objects: i32,
  tx: mpsc::Sender<usize>,
  rx: mpsc::Receiver<usize>,
  data: Vec<Arc<Mutex<Option<(LoadableObject)>>>>,
}

impl ResourceManager {
  pub fn new() -> ResourceManager {
    let (tx, rx) = mpsc::channel();
    
    ResourceManager {
      objects: Vec::new(),
      pool: ThreadPool::new(10),
      num_recv_objects: 0,
      tx: tx,
      rx: rx,
      data: Vec::new(),
    }
  }
  
  pub fn pending_objects_loaded(&self) -> bool {
    let mut result = false;
    if self.data.len() == 0 {
      result = true;
    }
    result
  }
  
  /**
  ** Needs to be called frequently in backend to move resources from unknown land to somewhere where we can use it
  **/
  pub fn recieve_objects(&mut self) {
    if self.num_recv_objects <= 0 {
      if self.data.len() > 0 {
        self.data.clear();
      }
      return;
    }
    
    let num = self.num_recv_objects;
    for _ in 0..num {
      match self.rx.try_recv() {
        Ok(i) => {
          let mut data = self.data[i].lock().unwrap();
          let object = data.take().unwrap();
          self.objects.push(object);
          self.num_recv_objects -= 1;
        },
        Err(_e) => { },
      }
    }
  }
  
  pub fn destroy(&self, device: Arc<Device>) {
    for object in &self.objects {
      match object {
        LoadableObject { loaded: true, location: _, reference, object_type } => {
          match object_type {
            ObjectType::Texture(some_image) => {
              if let Some(image) = some_image {
                image.destroy(Arc::clone(&device));
              }
            },
            ObjectType::Font(some_image) => {
              if let Some((font, image)) = some_image {
                image.destroy(Arc::clone(&device));
              }
            },
            ObjectType::Shape(some_image) => {
              if let Some((buffer, image)) = some_image {
                image.destroy(Arc::clone(&device));
              }
            },
            _ => {},
          }
        },
        _ => {},
      }
    }
  }
  
  fn get_unloaded_object(&mut self, reference: String) -> Option<LoadableObject> {
    let mut object = None;
    
    for i in 0..self.objects.len() {
      if self.objects[i].reference == reference {
        if !self.objects[i].loaded {
          object = Some(self.objects.remove(i));
          break;
        }
      }
    }
    object
  }
  
  pub fn remove_object(&mut self, reference: String) {
    for i in 0..self.objects.len() {
      if self.objects[i].reference == reference {
        self.objects.remove(i);
      }
    }
  }
  
  /**
  ** Returns None when resource isnt loaded yet otherwise returns a ImmutableImage of format R8G8B8A8Unorm thats already in memory.
  **/
  pub fn get_texture(&mut self, reference: String) -> Option<Image> {
    let mut result = None;
    
    for object in &self.objects {
      if object.reference == reference {
        match object.object_type {
          ObjectType::Texture(ref image) => {
            result = image.clone()
          },
          _ => {}
        }
      }
    }
    
    result
  }
  
  pub fn get_all_textures(&self) -> Vec<(String, Image)> {
    let mut result = Vec::with_capacity(self.objects.len());
    
    for object in &self.objects {
      let reference = object.reference.to_string();
      match object.object_type {
        ObjectType::Texture(ref image) => {
          if image.is_some() {
            result.push((reference, image.clone().unwrap()));
          }
        },
        _ => {}
      }
    }
    
    result
  }
  
  /**
  ** Inserts details for a texture, does not load the image into memory.
  ** Must call Load_texture as a DrawCall in order to use
  **/
  pub fn insert_unloaded_texture(&mut self, reference: String, location: String) {
    
    debug_assert!(self.check_object(reference.clone()), "Error, Object reference already exists!");
    
    self.objects.push(
      LoadableObject {
        loaded: false,
        location: location,
        reference: reference.clone(),
        object_type: ObjectType::Texture(None),
      }
    );
  }
  
  /**
  ** Inserts a image that was created elsewhere in the program into the resource manager, a location is not required here as it is presumed that it was not created from a file that the ResourceManager has access to.
  **/
  pub fn insert_texture(&mut self, reference: String, new_image: Image) {
        println!("inserting texture");
    debug_assert!(self.check_object(reference.clone()), "Error, Object reference already exists!");
    
    self.objects.push(
      LoadableObject {
        loaded: true,
        location: "".to_string(),
        reference: reference.clone(),
        object_type: ObjectType::Texture(Some(new_image)),
      }
    );
  }
  
  /**
  ** Forces thread to wait until resource is loaded into memory.
  **/
  pub fn sync_load_texture(&mut self, reference: String, location: String, device: Arc<Device>, instance: Arc<Instance>, command_pool: &CommandPool, queue: vk::Queue) {
    
    debug_assert!(self.check_object(reference.clone()), "Error, Object reference already exists!");
  
    let texture = ResourceManager::load_texture_into_memory(location.clone(), instance, device, command_pool, queue);
    
    self.objects.push(
      LoadableObject {
        loaded: true,
        location: location.clone(),
        reference: reference.clone(),
        object_type: ObjectType::Texture(Some(texture)),
      }
    );
  }
  /*
    /**
  ** Loads textures from inserted details in seperate threads, non bloacking.
  **/
  pub fn load_texture_from_reference(&mut self, reference: String, instance: Arc<Instance>, device: Arc<Device>, command_pool: &CommandPool, queue: vk::Queue) {
    debug_assert!(!self.check_object(reference.clone()), "Error: Object {} doesn't exist!", reference);
    
    let unloaded_object = self.get_unloaded_object(reference.clone());
    if let Some(object) = unloaded_object {
      let location = object.location;
      let reference = object.reference;
      
      self.load_texture(reference, location, instance, device, command_pool, queue);
    } else {
      println!("Object {} already loaded", reference);
    }
  }
  
  /**
  ** Loads textures in seperate threads, non bloacking.
  **/
  pub fn load_texture(&mut self, reference: String, location: String, instance: Arc<Instance>, device: Arc<Device>, command_pool: &CommandPool, queue: vk::Queue) {
    
    debug_assert!(self.check_object(reference.clone()), "Error: Object reference already exists!");
    println!("loading texture");
    self.num_recv_objects += 1;
    let index = self.data.len();
    
    self.data.push(Arc::new(Mutex::new(None)));
    
    let (data, tx) = (self.data[index].clone(), self.tx.clone());
    self.pool.execute(move || {
      let mut data = data.lock().unwrap();
      let texture = ResourceManager::load_texture_into_memory(location.clone(), instance, device, command_pool, queue);
      
      let object = LoadableObject {
        loaded: true,
        location: location,
        reference: reference,
        object_type: ObjectType::Texture(Some(texture)),
      };
      
      *data = Some((object));
      tx.send(index.clone()).unwrap();
    });
  }*/
  
  fn load_texture_into_memory(location: String, instance: Arc<Instance>, device: Arc<Device>, command_pool: &CommandPool, graphics_queue: vk::Queue) -> (Image) {
    let texture_start_time = time::Instant::now();
    
    let texture = Image::device_local(instance, device, location.to_string(), ImageType::Type2D, ImageViewType::Type2D, &vk::FORMAT_R8G8B8A8_UNORM, Sample::Count1Bit, ImageTiling::Optimal, command_pool, &graphics_queue);
    
    let texture_time = texture_start_time.elapsed().subsec_nanos() as f64 / 1000000000.0 as f64;
    println!("{} ms,  {:?}", (texture_time*1000f64) as f32, location);
    
    (texture)
  }
  
  fn check_object(&self, reference: String) -> bool {
    let mut result = true;
    for object in &self.objects {
      if object.reference == reference {
        result = false;
      }
    }
    result
  }
  /*
  /**
  ** Returns None when resource isnt loaded yet otherwise returns Vertex and Index buffers thats already in memory.
  **/
  pub fn get_shape(&mut self, reference: String) -> Option<(Arc<BufferAccess + Send + Sync>, Arc<ImmutableBuffer<[u32]>>)> {
    let mut result = None;
    
    for object in &self.objects {
      if object.reference == reference {
        match object.object_type {
          ObjectType::Shape(ref buffer) => {
            result = buffer.clone()
          },
          _ => {}
        }
      }
    }
    
    result
  }
  
  /**
  ** Inserts a shape (vertex + index) that was created elsewhere in the program into the resource manager
  **/
  pub fn insert_shape(&mut self, reference: String, shape_info: (Arc<BufferAccess + Send + Sync>, Arc<ImmutableBuffer<[u32]>>)) {
    
    debug_assert!(self.check_object(reference.clone()), "Error, Object reference already exists!");
    
    self.objects.push(
      LoadableObject {
        loaded: true,
        location: "".to_string(),
        reference: reference.clone(),
        object_type: ObjectType::Shape(Some(shape_info)),
      }
    );
  }
  
  /**
  ** Forces thread to wait until resource is loaded into memory.
  **/
  pub fn sync_load_shape(&mut self, reference: String, vertex: Vec<Vertex2d>, index: Vec<u32>, queue: Arc<Queue>) -> Vec<CommandBufferExecFuture<NowFuture, AutoCommandBuffer>> {
    
    debug_assert!(self.check_object(reference.clone()), "Error, Object reference already exists!");
    
    let (vertex, index, futures) = ResourceManager::load_shape_into_memory(reference.clone(), vertex, index, queue);
    
    self.objects.push(
      LoadableObject {
        loaded: true,
        location: "".to_string(),
        reference: reference.clone(),
        object_type: ObjectType::Shape(Some((vertex, index))),
      }
    );
    
    futures
  }
  
  /**
  ** Loads vertex and index in a seperate thread, non bloacking.
  **/
  pub fn load_shape(&mut self, reference: String, vertex: Vec<Vertex2d>, index: Vec<u32>, queue: Arc<Queue>) {
    println!("loading shape");
    debug_assert!(self.check_object(reference.clone()), "Error, Object reference already exists!");
    
    self.num_recv_objects += 1;
    let idx = self.data.len();
    
    self.data.push(Arc::new(Mutex::new(None)));
    
    let (data, tx) = (self.data[idx].clone(), self.tx.clone());
    self.pool.execute(move || {
      let mut data = data.lock().unwrap();
      let (vertex, index, futures) = ResourceManager::load_shape_into_memory(reference.clone(), vertex, index, queue);
      
      let object = LoadableObject {
        loaded: true,
        location: "".to_string(),
        reference: reference,
        object_type: ObjectType::Shape(Some((vertex, index))),
      };
      
      *data = Some((object, futures));
      tx.send(idx.clone()).unwrap();
    });
  }
  
  pub fn update_shape(&mut self, reference: String, vertex: Vec<Vertex2d>, index: Vec<u32>, queue: Arc<Queue>) -> Vec<CommandBufferExecFuture<NowFuture, AutoCommandBuffer>> {
    println!("updating shape");
    let (vertex, index, futures) = ResourceManager::load_shape_into_memory(reference.clone(), vertex, index, queue);
    
    let mut found = false;
    
    for i in 0..self.objects.len() {
      if self.objects[i].reference == reference {
        self.objects[i].object_type = ObjectType::Shape(Some((Arc::clone(&vertex), Arc::clone(&index))));
        found = true;
        break;
      }
    }
    
    if !found {
      self.insert_shape(reference, (vertex, index));
    }
    
    futures
  }
  */

  /*
  /**
  ** Returns None when resource isnt loaded yet otherwise returns font thats already in memory.
  **/
  pub fn get_font(&mut self, reference: String) -> Option<(GenericFont, Arc<ImmutableImage<format::R8G8B8A8Unorm>>)> {
    let mut result = None;
    
    for object in &self.objects {
      if object.reference == reference {
        match object.object_type {
          ObjectType::Font(ref font) => {
            result = font.clone()
          },
          _ => {}
        }
      }
    }
    
    result
  }
  
  /**
  ** Inserts the font details, and the location of the font texture, the texture is not loaded into memory until a load_font Drawcall is made.
  **
  ** Note: The font details will be in memory, even if it is unloaded, remove_font is recommended if space is required.
  **/
  pub fn insert_unloaded_font(&mut self, reference: String, location: String, _font: &[u8]) {
    
    debug_assert!(self.check_object(reference.clone()), "Error, Object reference already exists!");
    
    self.objects.push(
      LoadableObject {
        loaded: true,
        location: location,
        reference: reference.clone(),
        object_type: ObjectType::Font(None),
      }
    );
  }
  
  /**
  ** Inserts a font (GenericFont + Texture) that was created elsewhere in the program into the resource manager
  **/
  pub fn insert_font(&mut self, reference: String, font_info: (GenericFont, Arc<ImmutableImage<format::R8G8B8A8Unorm>>)) {
    
    debug_assert!(self.check_object(reference.clone()), "Error, Object reference already exists!");
    
    self.objects.push(
      LoadableObject {
        loaded: true,
        location: "".to_string(),
        reference: reference.clone(),
        object_type: ObjectType::Font(Some(font_info)),
      }
    );
  }
  
  /**
  ** Only way to laod new font, Forces thread to wait until resource is loaded into memory.
  **/
  pub fn sync_load_font(&mut self, reference: String, location: String, font: &[u8], queue: Arc<Queue>) -> CommandBufferExecFuture<NowFuture, AutoCommandBuffer> {
    
    debug_assert!(self.check_object(reference.clone()), "Error, Object reference already exists!");
    
    let (texture, futures) = ResourceManager::load_texture_into_memory(location.clone(), Arc::clone(&queue));
    let font = ResourceManager::load_font_into_memory(reference.clone(), font);
    
    self.objects.push(
      LoadableObject {
        loaded: true,
        location: location.clone(),
        reference: reference.clone(),
        object_type: ObjectType::Font(Some((font, texture))),
      }
    );
    
    futures
  }
  
  fn load_font_into_memory(reference: String, font: &[u8]) -> GenericFont {
    let font_start_time = time::Instant::now();
    
    let mut new_font = GenericFont::new();
    new_font.load_font(font);
    
    let font_time = font_start_time.elapsed().subsec_nanos() as f64 / 1000000000.0 as f64;
    println!("{} ms, Font: {:?}", (font_time*1000f64) as f32, reference);
    
    new_font
  }
  
  fn load_shape_into_memory(reference: String, vertex: Vec<Vertex2d>, index: Vec<u32>, queue: Arc<Queue>) -> (Arc<BufferAccess + Send + Sync>, Arc<ImmutableBuffer<[u32]>>, Vec<CommandBufferExecFuture<NowFuture, AutoCommandBuffer>>) {
    let shape_start_time = time::Instant::now();
    
    let (vertex, future_vtx) = ImmutableBuffer::from_iter(vertex.iter().cloned(),
                                                          BufferUsage::vertex_buffer(),
                                                          Arc::clone(&queue))
                                                          .expect("failed to create immutable vertex buffer");
                               
    let (index, future_idx) = ImmutableBuffer::from_iter(index.iter().cloned(),
                                                         BufferUsage::index_buffer(),
                                                         queue)
                                                         .expect("failed to create immutable index buffer");
    
    let shape_time = shape_start_time.elapsed().subsec_nanos() as f64 / 1000000000.0 as f64;
    println!("{} ms, Shape: {:?}", (shape_time*1000f64) as f32, reference);
    
    (vertex, index, vec!(future_vtx, future_idx))
  }
  
  fn load_texture_into_memory(location: String, queue: Arc<Queue>) -> (Arc<ImmutableImage<format::R8G8B8A8Unorm>>, CommandBufferExecFuture<NowFuture, AutoCommandBuffer>) {
    let texture_start_time = time::Instant::now();
    
    let (texture, tex_future) = {
      let image = image::open(&location.clone()).expect(&("No file or Directory at: ".to_string() + &location)).to_rgba(); 
      let (width, height) = image.dimensions();
      let image_data = image.into_raw().clone();
      
      vkimage::immutable::ImmutableImage::from_iter(
              image_data.iter().cloned(),
              vkimage::Dimensions::Dim2d { width: width, height: height },
              format::R8G8B8A8Unorm,
              queue).unwrap()
    };
    
    let texture_time = texture_start_time.elapsed().subsec_nanos() as f64 / 1000000000.0 as f64;
    println!("{} ms,  {:?}", (texture_time*1000f64) as f32, location);
    
    (texture, tex_future)
  }
  
  */
}
