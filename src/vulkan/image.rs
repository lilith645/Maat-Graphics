use vk;

use crate::vulkan::{Instance, Device};
use crate::vulkan::buffer::{Buffer, BufferUsage, CommandBuffer};
use crate::vulkan::pool::{CommandPool};
use crate::vulkan::vkenums::{ImageType, ImageViewType, ImageLayout, ImageTiling, ImageAspect, Sample, ImageUsage, SharingMode, PipelineStage, AccessFlagBits};
use crate::vulkan::check_errors;

use image;

use std::mem;
use std::ptr;

pub struct Image {
  image: vk::Image,
  image_view: vk::ImageView,
  memory: vk::DeviceMemory,
}

impl Image {
  pub fn device_local(instance: &Instance, device: &Device, location: String, image_type: ImageType, image_view_type: ImageViewType, format: &vk::Format, samples: Sample, tiling: ImageTiling, command_pool: &CommandPool, graphics_queue: &vk::Queue) -> Image {
    let image = image::open(&location.clone()).expect(&("No file or Directory at: ".to_string() + &location)).to_rgba(); 
    let (width, height) = image.dimensions();
    let image_data = image.into_raw().clone();
    
    let image_extent = vk::Extent3D { width: width, height: height, depth: 1 };
    
    let image_size: vk::DeviceSize = (width * height * 4).into();
    
    let mut texture_image: vk::Image = unsafe { mem::uninitialized() };
    let mut texture_memory: vk::DeviceMemory = unsafe { mem::uninitialized() };
    let mut texture_image_view: vk::ImageView = unsafe { mem::uninitialized() };
    
    let staging_usage = BufferUsage::transfer_src_buffer();
    let image_usage = ImageUsage::transfer_dst_sampled();
    
    let staging_buffer = Buffer::cpu_buffer(instance, device, staging_usage, 1, image_data);
    
    Image::create_image(instance, device, image_type, image_usage, format, &image_extent, samples, ImageLayout::Undefined, tiling, &mut texture_image, &mut texture_memory);
    
    Image::transition_layout(device, &texture_image, format, ImageLayout::Undefined, ImageLayout::TransferDstOptimal, command_pool, graphics_queue);
    
    let mut command_buffer = Image::begin_single_time_command(device, command_pool);
    command_buffer.copy_buffer_to_image(device, &staging_buffer, texture_image, ImageAspect::Colour, width, height, 0);
    Image::end_single_time_command(device, command_buffer, command_pool, graphics_queue);
    
    
    Image::transition_layout(device, &texture_image, format, ImageLayout::TransferDstOptimal, ImageLayout::ShaderReadOnlyOptimal, command_pool, graphics_queue);
    
    
    staging_buffer.destroy(device);
    
    texture_image_view = Image::create_image_view(device, &texture_image, format, image_view_type);
    
    Image {
      image: texture_image,
      image_view: texture_image_view,
      memory: texture_memory,
    }
  }
  
  pub fn get_image(&self) -> vk::Image {
    self.image
  }
  
  pub fn get_image_view(&self) -> vk::ImageView {
    self.image_view
  }
  
  pub fn get_image_memory(&self) -> vk::DeviceMemory {
    self.memory
  }
  
  fn begin_single_time_command(device: &Device, command_pool: &CommandPool) -> CommandBuffer {
    let command_buffer = CommandBuffer::primary(device, command_pool);
    command_buffer.begin_command_buffer(device, vk::COMMAND_BUFFER_LEVEL_PRIMARY);
    command_buffer
  }
  
  fn end_single_time_command(device: &Device, command_buffer: CommandBuffer, command_pool: &CommandPool, graphics_queue: &vk::Queue) {
    let submit_info = {
      vk::SubmitInfo {
        sType: vk::STRUCTURE_TYPE_SUBMIT_INFO,
        pNext: ptr::null(),
        waitSemaphoreCount: 0,
        pWaitSemaphores: ptr::null(),
        pWaitDstStageMask: ptr::null(),
        commandBufferCount: 1,
        pCommandBuffers: command_buffer.internal_object(),
        signalSemaphoreCount: 0,
        pSignalSemaphores: ptr::null(),
      }
    };
    
    command_buffer.end_command_buffer(device);
    
    unsafe {
      let vk = device.pointers();
      let device = device.internal_object();
      let command_pool = command_pool.local_command_pool();
      vk.QueueSubmit(*graphics_queue, 1, &submit_info, 0);
      vk.QueueWaitIdle(*graphics_queue);
      vk.FreeCommandBuffers(*device, *command_pool, 1, command_buffer.internal_object());
    }
  }
  
  fn transition_layout(device: &Device, image: &vk::Image, format: &vk::Format, old_layout: ImageLayout, new_layout: ImageLayout, command_pool: &CommandPool, graphics_queue: &vk::Queue) {
    
    let subresource_range = vk::ImageSubresourceRange {
      aspectMask: ImageAspect::Colour.to_bits(),
      baseMipLevel: 0,
      levelCount: 1,
      baseArrayLayer: 0,
      layerCount: 1,
    };
    
    let mut src_stage: PipelineStage;
    let mut dst_stage: PipelineStage;
    let mut src_access: Option<AccessFlagBits> = None;
    let mut dst_access: AccessFlagBits;
    
    if old_layout == ImageLayout::Undefined && new_layout == ImageLayout::TransferDstOptimal {
      dst_access = AccessFlagBits::TransferWrite;
      
      src_stage = PipelineStage::TopOfPipe;
      dst_stage = PipelineStage::Transfer;
    } else if old_layout == ImageLayout::TransferDstOptimal && new_layout == ImageLayout::ShaderReadOnlyOptimal {
      src_access = Some(AccessFlagBits::TransferWrite);
      dst_access = AccessFlagBits::ShaderRead;
      
      src_stage = PipelineStage::Transfer;
      dst_stage = PipelineStage::FragmentShader;
    } else {
      panic!("Error image transition not supported!");
    }
    
    let barrier = vk::ImageMemoryBarrier {
      sType: vk::STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER,
      pNext: ptr::null(),
      srcAccessMask: if src_access.is_some() { src_access.unwrap().to_bits() } else { 0 },
      dstAccessMask: dst_access.to_bits(),
      oldLayout: old_layout.to_bits(),
      newLayout: new_layout.to_bits(),
      srcQueueFamilyIndex: vk::QUEUE_FAMILY_IGNORED,
      dstQueueFamilyIndex: vk::QUEUE_FAMILY_IGNORED,
      image: *image,
      subresourceRange: subresource_range,
    };
    
    let mut command_buffer = Image::begin_single_time_command(device, command_pool);
    command_buffer.pipeline_barrier(device, src_stage, dst_stage, barrier);
    Image::end_single_time_command(device, command_buffer, command_pool, graphics_queue);
  }
  
  fn create_image(instance: &Instance, device: &Device, image_type: ImageType, usage: ImageUsage, format: &vk::Format, image_extent: &vk::Extent3D, samples: Sample, initial_layout: ImageLayout, tiling: ImageTiling, image: &mut vk::Image, image_memory: &mut vk::DeviceMemory) {
    
    let vk = device.pointers();
    let vk_instance = instance.pointers();
    let phys_device = device.physical_device();
    let device = device.internal_object();
    
    let image_create_info = {
      vk::ImageCreateInfo {
        sType: vk::STRUCTURE_TYPE_IMAGE_CREATE_INFO,
        pNext: ptr::null(),
        flags: 0,
        imageType: image_type.to_bits(),
        format: *format,
        extent: vk::Extent3D { width: image_extent.width, height: image_extent.height, depth: 1 },
        mipLevels: 1,
        arrayLayers: 1,
        samples: samples.to_bits(),
        tiling: tiling.to_bits(),
        usage: usage.to_bits(),
        sharingMode: SharingMode::Exclusive.to_bits(),
        queueFamilyIndexCount: 0,
        pQueueFamilyIndices: ptr::null(),
        initialLayout: initial_layout.to_bits(),
      }
    };
    
   let mut memory_requirements: vk::MemoryRequirements = unsafe { mem::uninitialized() };
    
    unsafe {
      check_errors(vk.CreateImage(*device, &image_create_info, ptr::null(), image));
      vk.GetImageMemoryRequirements(*device, *image, &mut memory_requirements);
    }
    
    let memory_type_bits_index = {
      
      let mut memory_properties: vk::PhysicalDeviceMemoryProperties = unsafe { mem::uninitialized() };
      
      unsafe {
        vk_instance.GetPhysicalDeviceMemoryProperties(*phys_device, &mut memory_properties);
      }
      
      let mut index: i32 = -1;
      let properties = vk::MEMORY_PROPERTY_DEVICE_LOCAL_BIT;
      for i in 0..memory_properties.memoryTypeCount as usize {
        if memory_requirements.memoryTypeBits & (1 << i) != 0 && memory_properties.memoryTypes[i].propertyFlags & properties == properties {
          index = i as i32;
        }
      }
      
      if index == -1 {
        panic!("Failed to find suitable memory type");
      }
      
      index
    };
    
    let memory_allocate_info = {
      vk::MemoryAllocateInfo {
        sType: vk::STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
        pNext: ptr::null(),
        allocationSize: memory_requirements.size,
        memoryTypeIndex: memory_type_bits_index as u32,
      }
    };
    
    unsafe {
      check_errors(vk.AllocateMemory(*device, &memory_allocate_info, ptr::null(), image_memory));
      check_errors(vk.BindImageMemory(*device, *image, *image_memory, 0));
    }
  }
  
  fn create_image_view(device: &Device, image: &vk::Image, format: &vk::Format, image_view_type: ImageViewType) -> vk::ImageView {
    let vk = device.pointers();
    let device = device.internal_object();
    
    let mut image_view: vk::ImageView = unsafe { mem::uninitialized() };
    
    let component = vk::ComponentMapping {
      r: vk::COMPONENT_SWIZZLE_IDENTITY,
      g: vk::COMPONENT_SWIZZLE_IDENTITY,
      b: vk::COMPONENT_SWIZZLE_IDENTITY,
      a: vk::COMPONENT_SWIZZLE_IDENTITY,
    };
    
    let subresource = vk::ImageSubresourceRange {
      aspectMask: vk::IMAGE_ASPECT_COLOR_BIT,
      baseMipLevel: 0,
      levelCount: 1,
      baseArrayLayer: 0,
      layerCount: 1,
    };
    
    let image_view_create_info = vk::ImageViewCreateInfo {
      sType: vk::STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO,
      pNext: ptr::null(),
      flags: 0,
      image: *image,
      viewType: image_view_type.to_bits(),
      format: *format,
      components: component,
      subresourceRange: subresource,
    };
    
    unsafe {
      vk.CreateImageView(*device, &image_view_create_info, ptr::null(), &mut image_view);
    }
    
    image_view
  }
  
  pub fn destroy(&self, device: &Device) {
    unsafe {
      let vk = device.pointers();
      let device = device.internal_object();
      
      vk.DestroyImageView(*device, self.image_view, ptr::null());
      vk.DestroyImage(*device, self.image, ptr::null());
      vk.FreeMemory(*device, self.memory, ptr::null());
    }
  }
}