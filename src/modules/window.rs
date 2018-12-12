use vk;
use winit;
use loader;
use loader::Loader;
use loader::FunctionPointers;
use modules::Swapchain;
use modules::Instance;
use modules::Device;

use std::ptr;
use std::mem;
use std::ffi::CStr;
use std::ffi::CString;

use std::borrow::Borrow;

use winit::dpi::LogicalSize;

use ownage::OwnedOrRef;
use ownage::check_errors;

use ENGINE_VERSION;

#[cfg(all(unix, not(target_os = "android"), not(target_os = "macos")))]
unsafe fn create_surface(
    instance: &Instance,
    window: &winit::Window,
) -> vk::SurfaceKHR {
  use winit::os::unix::WindowExt;
  
  let vk = instance.pointers();
  let extensions = instance.get_extensions();
  let instance = instance.local_instance();
  
  match (window.borrow().get_wayland_display(), window.borrow().get_wayland_surface()) {
    (Some(display), Some(surface)) => {//wayland
      if !extensions.contains(&CString::new("VK_KHR_wayland_surface").unwrap()) {
        panic!("Missing extension VK_KHR_wayland_surface");
      }
      
      let surface = {
        let infos = vk::WaylandSurfaceCreateInfoKHR {
            sType: vk::STRUCTURE_TYPE_WAYLAND_SURFACE_CREATE_INFO_KHR,
            pNext: ptr::null(),
            flags: 0, // reserved
            display: display as *mut _,
            surface: surface as *mut _,
        };
        let mut output = mem::uninitialized();
        check_errors(vk.CreateWaylandSurfaceKHR(*instance,
                                                &infos,
                                                ptr::null(),
                                                &mut output));
        output
      };
      surface
    },
    _ => {
      //xlib
      if !extensions.contains(&CString::new("VK_KHR_xlib_surface").unwrap()) {
        let surface = {
          let infos = vk::XlibSurfaceCreateInfoKHR {
            sType: vk::STRUCTURE_TYPE_XLIB_SURFACE_CREATE_INFO_KHR,
            pNext: ptr::null(),
            flags: 0, // reserved
            dpy: window.borrow().get_xlib_display().unwrap() as *mut _,
            window: window.borrow().get_xlib_window().unwrap() as _,
          };

          let mut output = mem::uninitialized();
          check_errors(vk.CreateXlibSurfaceKHR(*instance,
                                               &infos,
                                               ptr::null(),
                                               &mut output));
          output
        };
        
        surface
      } else {//xcb
        if !extensions.contains(&CString::new("VK_KHR_xcb_surface").unwrap()) {
          panic!("Missing extension VK_KHR_xcb_surface");
        }
        
        let surface = {
          let infos = vk::XcbSurfaceCreateInfoKHR {
            sType: vk::STRUCTURE_TYPE_XCB_SURFACE_CREATE_INFO_KHR,
            pNext: ptr::null(),
            flags: 0, // reserved
            connection: window.borrow().get_xcb_connection().unwrap() as *mut _,
            window: window.borrow().get_xlib_window().unwrap() as _,
          };

          let mut output = mem::uninitialized();
          check_errors(vk.CreateXcbSurfaceKHR(*instance,
                                              &infos,
                                              ptr::null(),
                                              &mut output));
          output
        };
        
        surface
      }
    }
  }
}

pub struct VkWindow {
  instance: Instance,
  device: Device,
  surface: vk::SurfaceKHR,
  swapchain: Swapchain,
  graphics_queue: vk::Queue,
  present_queue: vk::Queue,
  graphics_present_family_index: (u32, u32),
  window: winit::Window,
  events_loop: winit::EventsLoop,
}

impl VkWindow {
  pub fn new(app_name: String, app_version: u32, width: f32, height: f32, should_debug: bool) -> VkWindow {
    let function_pointers = OwnedOrRef::Ref(loader::auto_loader().unwrap());
    let entry_points = function_pointers.entry_points();
    
    let instance = Instance::new(app_name.to_string(), app_version, should_debug);
    
    let (window, events_loop, surface) = {
      VkWindow::create_window(&instance,
                              app_name, 
                              width, 
                              height)
    };
    
    let device = Device::new(&instance, &surface);
    
    let (graphics_family, present_family, graphics_queue, present_queue) = VkWindow::find_queue_families(&instance, &device, &surface);
    
    let swapchain = Swapchain::new(&instance, &device, &surface, graphics_family, present_family);
    
    VkWindow {
      instance: instance,
      device: device,
      surface: surface,
      swapchain: swapchain,
      graphics_queue: graphics_queue,
      present_queue: present_queue,
      graphics_present_family_index: (graphics_family, present_family),
      window: window,
      events_loop: events_loop,
    }
  }
  
  pub fn get_current_extent(&self) -> vk::Extent2D {
    self.get_capabilities().currentExtent
  }
  /*
  pub fn recreate_swapchain_images(&mut self, window_dimensions: &vk::Extent2D) {
    let (graphics_family, present_family, graphics_queue, present_queue) = VkWindow::find_queue_families(&self.instance, &self.vk_device, &self.device, &self.phys_device, &self.surface);
    self.swapchain.recreate_swapchain_images(&self.instance, &self.vk_device, &self.device, &self.phys_device, &self.surface, graphics_family, present_family);
  }*/
  
  pub fn get_swapchain(&self) -> &vk::SurfaceKHR {
    self.swapchain.get_swapchain()
  }
  
  pub fn swapchain_image_views(&self) -> &Vec<vk::ImageView> {
    self.swapchain.get_image_views()
  }
  
  pub fn swapchain_format(&self) -> vk::Format {
    self.swapchain.get_format()
  }
  
  pub fn get_events(&mut self) -> &mut winit::EventsLoop {
    &mut self.events_loop
  }
  
  pub fn instance_pointers(&self) -> &vk::InstancePointers {
    &self.instance.pointers()
  } 
  
  pub fn device_pointers(&self) -> &vk::DevicePointers {
    &self.device.pointers()
  }
  
  pub fn instance(&self) -> &Instance {
    &self.instance
  }
  
  pub fn device(&self) -> &Device {
    &self.device
  }
  
  pub fn physical_device(&self) -> &vk::PhysicalDevice {
    &self.device.physical_device()
  }
  
  pub fn get_graphics_queue(&self) -> &vk::Queue {
    &self.graphics_queue
  }
  
  pub fn get_present_queue(&self) -> &vk::Queue {
    &self.present_queue
  }
  
  pub fn get_graphics_family(&self) -> u32 {
    self.graphics_present_family_index.0
  }
  
  fn get_capabilities(&self) -> vk::SurfaceCapabilitiesKHR {
    let phys_device = self.device.physical_device();
    self.instance.get_surface_capabilities(phys_device, &self.surface)
  }
  
  fn create_window(instance: &Instance, app_name: String, width: f32, height: f32) -> (winit::Window, winit::EventsLoop, vk::SurfaceKHR) {
    let events_loop = winit::EventsLoop::new();
    let window = winit::WindowBuilder::new().with_title(app_name).with_dimensions(LogicalSize::new(width as f64, height as f64)).build(&events_loop).unwrap();
    
    let surface = unsafe { create_surface(&instance, &window) };
    
    (window, events_loop, surface)
  }
  
  fn find_queue_families(instance: &Instance, device: &Device, surface: &vk::SurfaceKHR) -> (u32, u32, vk::Queue, vk::Queue) {
    let vk = device.pointers();
    let phys_device = device.physical_device();
    
    let queue_family_properties: Vec<vk::QueueFamilyProperties> = instance.get_queue_family_properties(phys_device);
    
    let mut graphics_family: i32 = -1;
    let mut present_family: i32 = -1;
    
    for i in 0..queue_family_properties.len() {
      let queue_family = &queue_family_properties[i];
      if queue_family.queueCount > 0 && VkWindow::has_graphics_bit(&queue_family.queueFlags) {
        graphics_family = i as i32;
      }
      
      let mut present_supported = instance.get_supported_display_queue_families(phys_device, surface, i as u32);
      
      if queue_family.queueCount > 0 && present_supported != 0 {
         present_family = i as i32;
      }
      
      if graphics_family > 0 && present_family > 0 {
        break;
      }
    }
    
    let graphics_queue: vk::Queue = device.get_device_queue(graphics_family as u32, 0);
    let present_queue: vk::Queue = device.get_device_queue(present_family as u32, 0);
    
    (graphics_family as u32, present_family as u32, graphics_queue, present_queue)
  }
  
  fn create_instance(entry_points: &vk::EntryPoints, function_pointers: &OwnedOrRef<FunctionPointers<Box<dyn Loader + Sync + Send>>>, app_name: String, app_version: u32, should_debug: bool, supported_extensions: Vec<CString>) -> (vk::InstancePointers, vk::Instance, Vec<CString>, Vec<CString>) {
    let app_name = CString::new(app_name).unwrap();
    let engine_name = CString::new("Maat-Graphics").unwrap();
    
    let layer_names = {
      if should_debug {
        [CString::new("VK_LAYER_LUNARG_standard_validation").unwrap()]
      } else {
        [CString::new("").unwrap()]
      }
    };
    let layers_names_raw: Vec<*const i8> = layer_names.iter().map(|raw_name| raw_name.as_ptr()).collect();
    
    let ideal_extension_names: [CString; 9] = [
      CString::new("VK_KHR_surface").unwrap(),
      CString::new("VK_KHR_xlib_surface").unwrap(),
      CString::new("VK_KHR_xcb_surface").unwrap(),
      CString::new("VK_KHR_wayland_surface").unwrap(),
      CString::new("VK_KHR_android_surface").unwrap(),
      CString::new("VK_KHR_win32_surface").unwrap(),
      CString::new("VK_MVK_ios_surface").unwrap(),
      CString::new("VK_MVK_macos_surface").unwrap(),
      CString::new("VK_EXT_debug_utils").unwrap(),
    ];
    
    let mut available_extensions = Vec::new();
    for supported_extension in &supported_extensions {
      for ideal_extension in &ideal_extension_names {
        if ideal_extension == supported_extension {
          available_extensions.push(supported_extension.clone());
        }
      }
    }
    
    let available_extensions_raw: Vec<*const i8> = available_extensions.iter().map(|raw_name| raw_name.as_ptr()).collect();
    
    let appinfo = vk::ApplicationInfo {
      pApplicationName: app_name.as_ptr(),
      sType: vk::STRUCTURE_TYPE_APPLICATION_INFO,
      pNext: ptr::null(),
      applicationVersion: app_version,
      pEngineName: engine_name.as_ptr(),
      engineVersion: ENGINE_VERSION,
      apiVersion: (1 as u32) << 22 | (0 as u32) << 12 | (5 as u32),
    };
    
    let instance: vk::Instance = unsafe {
      let mut output = mem::uninitialized();
      let instance_info = vk::InstanceCreateInfo {
        sType: vk::STRUCTURE_TYPE_INSTANCE_CREATE_INFO,
        pNext: ptr::null(),
        flags: Default::default(),
        pApplicationInfo: &appinfo,
        ppEnabledLayerNames: layers_names_raw.as_ptr(),
        enabledLayerCount: layers_names_raw.len() as u32,
        ppEnabledExtensionNames: available_extensions_raw.as_ptr(),
        enabledExtensionCount: available_extensions_raw.len() as u32,
      };
      
      check_errors(entry_points.CreateInstance(&instance_info, ptr::null(), &mut output));
      
      output
    };
    
    let vk_instance = {
      vk::InstancePointers::load(|name| unsafe {
        mem::transmute(function_pointers.get_instance_proc_addr(instance, name.as_ptr()))
      })
    };
    
    (vk_instance, instance, available_extensions, layer_names.to_vec())
  }
  
  fn has_graphics_bit(queue_flags: &u32) -> bool {
    queue_flags % 2 != 0 
  }
}

impl Drop for VkWindow {
  fn drop(&mut self) {
    self.device.wait();
    self.swapchain.destroy(&self.device);
    self.device.destroy();
    self.instance.destroy();
  }
}
