use vk;

use crate::vulkan::vkenums::{SampleCount, AttachmentLoadOp, AttachmentStoreOp, ImageLayout, PipelineBindPoint, PipelineStage, Access, Dependency};

use crate::vulkan::Device;

use std::mem;
use std::ptr;
use std::sync::Arc;

#[derive(Clone)]
pub struct RenderPass {
  render_pass: vk::RenderPass,
  num_attachments: u32,
}

impl RenderPass {
  pub fn new_from_renderpass(render_pass: vk::RenderPass, num_attachments: u32) -> RenderPass {
    RenderPass {
      render_pass,
      num_attachments,
    }
  }
  
  pub fn new(device: Arc<Device>, format: &vk::Format) -> RenderPass {
    let mut render_pass: vk::RenderPass = unsafe { mem::uninitialized() };
    
    let mut attachment_description = Vec::with_capacity(1);
    attachment_description.push(
      vk::AttachmentDescription {
        flags: 0,
        format: *format,
        samples: SampleCount::OneBit.to_bits(),
        loadOp: AttachmentLoadOp::Clear.to_bits(),
        storeOp: AttachmentStoreOp::Store.to_bits(),
        stencilLoadOp: AttachmentLoadOp::DontCare.to_bits(),
        stencilStoreOp: AttachmentLoadOp::DontCare.to_bits(),
        initialLayout: ImageLayout::Undefined.to_bits(),
        finalLayout: ImageLayout::PresentSrcKHR.to_bits(),
      }
    );
    
   // let mut input_attachments: Vec<vk::AttachmentReference>;
    let mut colour_attachments: Vec<vk::AttachmentReference> = Vec::new();
    //let mut resolve_attachmets: Vec<vk::AttachmentReference>;
    
    colour_attachments.push(
      vk::AttachmentReference {
        attachment: 0,
        layout: ImageLayout::ColourAttachmentOptimal.to_bits(),
      }
    );
    
    let mut subpass_description = Vec::with_capacity(1);
    subpass_description.push(
      vk::SubpassDescription {
        flags: 0,
        pipelineBindPoint: PipelineBindPoint::Graphics.to_bits(),
        inputAttachmentCount: 0,//input_attachments.len() as u32,
        pInputAttachments: ptr::null(),//input_attachments,
        colorAttachmentCount: colour_attachments.len() as u32,
        pColorAttachments: colour_attachments.as_ptr(),
        pResolveAttachments: ptr::null(),//resolve_attachmets.len() as u32,
        pDepthStencilAttachment: ptr::null(),//resolve_attachmets,
        preserveAttachmentCount: 0,
        pPreserveAttachments: ptr::null(),
      }
    );
    
    let mut subpass_dependency: Vec<vk::SubpassDependency> = Vec::with_capacity(2);
    
    subpass_dependency.push(vk::SubpassDependency {
      srcSubpass: vk::SUBPASS_EXTERNAL,
      dstSubpass: 0,
      srcStageMask: PipelineStage::ColorAttachmentOutput.to_bits(),
      dstStageMask: PipelineStage::ColorAttachmentOutput.to_bits(),
      srcAccessMask: 0,
      dstAccessMask: Access::ColourAttachmentRead.to_bits() | Access::ColourAttachmentWrite.to_bits(),
      dependencyFlags: Dependency::ByRegion.to_bits(),
    });
    
    subpass_dependency.push(vk::SubpassDependency {
      srcSubpass: 0,
      dstSubpass: vk::SUBPASS_EXTERNAL,
      srcStageMask: PipelineStage::ColorAttachmentOutput.to_bits(),
      dstStageMask: PipelineStage::BottomOfPipe.to_bits(),
      srcAccessMask: Access::ColourAttachmentRead.to_bits() | Access::ColourAttachmentWrite.to_bits(),
      dstAccessMask: 0,
      dependencyFlags: Dependency::ByRegion.to_bits(),
    });
    
    let render_pass_create_info = vk::RenderPassCreateInfo {
      sType: vk::STRUCTURE_TYPE_RENDER_PASS_CREATE_INFO,
      pNext: ptr::null(),
      flags: 0,
      attachmentCount: attachment_description.len() as u32,
      pAttachments: attachment_description.as_ptr(),
      subpassCount: subpass_description.len() as u32,
      pSubpasses: subpass_description.as_ptr(),
      dependencyCount: subpass_dependency.len() as u32,
      pDependencies: subpass_dependency.as_ptr(),
    };
    
    let vk = device.pointers();
    let device = device.internal_object();
    
    unsafe {
      vk.CreateRenderPass(*device, &render_pass_create_info, ptr::null(), &mut render_pass);
    }
    
    RenderPass {
      render_pass,
      num_attachments: 1,
    }
  }
  
  pub fn internal_object(&self) -> &vk::RenderPass {
    &self.render_pass
  }
  
  pub fn get_num_attachments(&self) -> u32 {
    self.num_attachments
  }
  
  pub fn destroy(&self, device: Arc<Device>) {
    let vk = device.pointers();
    let device = device.internal_object();
    
    println!("Destroying RenderPass");
    
    unsafe {
      vk.DestroyRenderPass(*device, self.render_pass, ptr::null());
    }
  }
}
