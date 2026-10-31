use super::context::Context;
use anyhow::Result;
use ash::vk;
use std::{ptr::NonNull, sync::Arc};

pub struct Buffer {
    pub handle: vk::Buffer,
    pub size: u64,
    memory: vk::DeviceMemory,
    pub mapped: Option<NonNull<u8>>,
    ctx: Arc<Context>,
}
impl Buffer {
    pub fn new(
        ctx: &Arc<Context>,
        size: u64,
        usage: vk::BufferUsageFlags,
        host: bool,
    ) -> Result<Self> {
        unsafe {
            let handle = ctx.device.create_buffer(
                &vk::BufferCreateInfo::default()
                    .size(size.max(16))
                    .usage(usage)
                    .sharing_mode(vk::SharingMode::EXCLUSIVE),
                None,
            )?;
            let req = ctx.device.get_buffer_memory_requirements(handle);
            let flags = if host {
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT
            } else {
                vk::MemoryPropertyFlags::DEVICE_LOCAL
            };
            let mut address_flags = vk::MemoryAllocateFlagsInfo::default()
                .flags(vk::MemoryAllocateFlags::DEVICE_ADDRESS);
            let mut allocation = vk::MemoryAllocateInfo::default()
                .allocation_size(req.size)
                .memory_type_index(ctx.memory_type(req.memory_type_bits, flags)?);
            if usage.contains(vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS) {
                allocation = allocation.push_next(&mut address_flags);
            }
            let memory = ctx.device.allocate_memory(&allocation, None)?;
            ctx.device.bind_buffer_memory(handle, memory, 0)?;
            let mapped = if host {
                NonNull::new(ctx.device.map_memory(
                    memory,
                    0,
                    req.size,
                    vk::MemoryMapFlags::empty(),
                )? as *mut u8)
            } else {
                None
            };
            Ok(Self {
                handle,
                size: size.max(16),
                memory,
                mapped,
                ctx: ctx.clone(),
            })
        }
    }
    pub fn write(&self, offset: usize, bytes: &[u8]) -> Result<()> {
        anyhow::ensure!(
            offset + bytes.len() <= self.size as usize,
            "Buffer upload exceeds capacity"
        );
        let pointer = self
            .mapped
            .ok_or_else(|| anyhow::anyhow!("Buffer is not mapped"))?;
        unsafe {
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                pointer.as_ptr().add(offset),
                bytes.len(),
            );
        }
        Ok(())
    }
}
impl Drop for Buffer {
    fn drop(&mut self) {
        unsafe {
            if self.mapped.is_some() {
                self.ctx.device.unmap_memory(self.memory);
            }
            self.ctx.device.destroy_buffer(self.handle, None);
            self.ctx.device.free_memory(self.memory, None);
        }
    }
}

pub struct Image {
    pub handle: vk::Image,
    pub view: vk::ImageView,
    pub _levels: u32,
    memory: vk::DeviceMemory,
    ctx: Arc<Context>,
}
impl Image {
    // Mirrors the explicit Vulkan image creation fields.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        ctx: &Arc<Context>,
        width: u32,
        height: u32,
        format: vk::Format,
        usage: vk::ImageUsageFlags,
        levels: u32,
        samples: vk::SampleCountFlags,
        aspect: vk::ImageAspectFlags,
    ) -> Result<Self> {
        unsafe {
            let handle = ctx.device.create_image(
                &vk::ImageCreateInfo::default()
                    .image_type(vk::ImageType::TYPE_2D)
                    .format(format)
                    .extent(vk::Extent3D {
                        width,
                        height,
                        depth: 1,
                    })
                    .mip_levels(levels)
                    .array_layers(1)
                    .samples(samples)
                    .tiling(vk::ImageTiling::OPTIMAL)
                    .usage(usage)
                    .sharing_mode(vk::SharingMode::EXCLUSIVE),
                None,
            )?;
            let req = ctx.device.get_image_memory_requirements(handle);
            let memory = ctx.device.allocate_memory(
                &vk::MemoryAllocateInfo::default()
                    .allocation_size(req.size)
                    .memory_type_index(ctx.memory_type(
                        req.memory_type_bits,
                        vk::MemoryPropertyFlags::DEVICE_LOCAL,
                    )?),
                None,
            )?;
            ctx.device.bind_image_memory(handle, memory, 0)?;
            let view = ctx.device.create_image_view(
                &vk::ImageViewCreateInfo::default()
                    .image(handle)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(format)
                    .subresource_range(
                        vk::ImageSubresourceRange::default()
                            .aspect_mask(aspect)
                            .level_count(levels)
                            .layer_count(1),
                    ),
                None,
            )?;
            Ok(Self {
                handle,
                view,
                _levels: levels,
                memory,
                ctx: ctx.clone(),
            })
        }
    }
}
impl Drop for Image {
    fn drop(&mut self) {
        unsafe {
            self.ctx.device.destroy_image_view(self.view, None);
            self.ctx.device.destroy_image(self.handle, None);
            self.ctx.device.free_memory(self.memory, None);
        }
    }
}

// Keep both sides of a Vulkan dependency visible at each call site.
#[allow(clippy::too_many_arguments)]
pub fn image_barrier(
    ctx: &Context,
    cmd: vk::CommandBuffer,
    image: vk::Image,
    old: vk::ImageLayout,
    new: vk::ImageLayout,
    aspect: vk::ImageAspectFlags,
    base: u32,
    count: u32,
    src: vk::AccessFlags,
    dst: vk::AccessFlags,
    from: vk::PipelineStageFlags,
    to: vk::PipelineStageFlags,
) {
    unsafe {
        let barrier = vk::ImageMemoryBarrier::default()
            .old_layout(old)
            .new_layout(new)
            .image(image)
            .src_access_mask(src)
            .dst_access_mask(dst)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .subresource_range(
                vk::ImageSubresourceRange::default()
                    .aspect_mask(aspect)
                    .base_mip_level(base)
                    .level_count(count)
                    .layer_count(1),
            );
        ctx.device.cmd_pipeline_barrier(
            cmd,
            from,
            to,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[barrier],
        );
    }
}
