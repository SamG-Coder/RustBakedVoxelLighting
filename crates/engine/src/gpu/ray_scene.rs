//! Immutable acceleration structures. One BLAS per shared mesh, one TLAS for placements.
use super::{Buffer, Context, immediate};
use crate::assets::*;
use anyhow::Result;
use ash::vk;
use std::sync::Arc;
struct Acceleration {
    handle: vk::AccelerationStructureKHR,
    _storage: Buffer,
}
pub struct RayScene {
    ctx: Arc<Context>,
    api: ash::khr::acceleration_structure::Device,
    structures: Vec<Acceleration>,
    pub metadata: Buffer,
}
impl RayScene {
    pub fn handle(&self) -> vk::AccelerationStructureKHR {
        self.structures.last().unwrap().handle
    }
    pub fn build(
        ctx: &Arc<Context>,
        pool: vk::CommandPool,
        scene: &SceneData,
        vertices: &Buffer,
        indices: &Buffer,
    ) -> Result<Self> {
        let start = std::time::Instant::now();
        let api = ash::khr::acceleration_structure::Device::new(&ctx.instance, &ctx.device);
        let mut result = Self {
            ctx: ctx.clone(),
            api,
            structures: Vec::with_capacity(scene.ray_meshes.len() + 1),
            metadata: Buffer::new(
                ctx,
                (scene.ray_meshes.len() * 16) as u64,
                vk::BufferUsageFlags::STORAGE_BUFFER,
                true,
            )?,
        };
        unsafe {
            let address = |b: &Buffer| {
                ctx.device.get_buffer_device_address(
                    &vk::BufferDeviceAddressInfo::default().buffer(b.handle),
                )
            };
            let va = address(vertices);
            let ia = address(indices);
            let mut geometries = Vec::with_capacity(scene.ray_meshes.len());
            let mut counts = Vec::new();
            let mut scratch_max = 0;
            let mut meta = Vec::new();
            for item in &scene.ray_meshes {
                let m = item.mesh;
                let max_vertex = scene.indices
                    [m.first_index as usize..(m.first_index + m.index_count) as usize]
                    .iter()
                    .copied()
                    .max()
                    .unwrap_or(0);
                let triangles = vk::AccelerationStructureGeometryTrianglesDataKHR::default()
                    .vertex_format(vk::Format::R32G32B32_SFLOAT)
                    .vertex_data(vk::DeviceOrHostAddressConstKHR {
                        device_address: va + m.vertex_offset as u64 * 56,
                    })
                    .vertex_stride(56)
                    .max_vertex(max_vertex)
                    .index_type(vk::IndexType::UINT32)
                    .index_data(vk::DeviceOrHostAddressConstKHR {
                        device_address: ia + m.first_index as u64 * 4,
                    });
                let transparent = scene.materials[item.material as usize].settings[0] == 1.;
                let geometry = vk::AccelerationStructureGeometryKHR::default()
                    .geometry_type(vk::GeometryTypeKHR::TRIANGLES)
                    .geometry(vk::AccelerationStructureGeometryDataKHR { triangles })
                    .flags(if transparent {
                        vk::GeometryFlagsKHR::empty()
                    } else {
                        vk::GeometryFlagsKHR::OPAQUE
                    });
                let info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
                    .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
                    .flags(vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE)
                    .geometries(std::slice::from_ref(&geometry));
                let count = m.index_count / 3;
                let mut size = vk::AccelerationStructureBuildSizesInfoKHR::default();
                result.api.get_acceleration_structure_build_sizes(
                    vk::AccelerationStructureBuildTypeKHR::DEVICE,
                    &info,
                    &[count],
                    &mut size,
                );
                scratch_max = scratch_max.max(size.build_scratch_size);
                result.structures.push(result.allocate(
                    size.acceleration_structure_size,
                    vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL,
                )?);
                geometries.push(geometry);
                counts.push(count);
                meta.extend_from_slice(&[m.first_index, m.vertex_offset as u32, item.material, 0]);
            }
            result.metadata.write(0, bytemuck::cast_slice(&meta))?;
            let mut properties = vk::PhysicalDeviceAccelerationStructurePropertiesKHR::default();
            ctx.instance.get_physical_device_properties2(
                ctx.physical,
                &mut vk::PhysicalDeviceProperties2::default().push_next(&mut properties),
            );
            let alignment = properties.min_acceleration_structure_scratch_offset_alignment as u64;
            let scratch = Buffer::new(
                ctx,
                scratch_max + alignment,
                vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
                false,
            )?;
            let scratch_address = address(&scratch).div_ceil(alignment) * alignment;
            immediate(ctx, pool, |cmd| {
                for i in 0..geometries.len() {
                    let info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
                        .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
                        .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
                        .flags(vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE)
                        .dst_acceleration_structure(result.structures[i].handle)
                        .geometries(std::slice::from_ref(&geometries[i]))
                        .scratch_data(vk::DeviceOrHostAddressKHR {
                            device_address: scratch_address,
                        });
                    let range = [vk::AccelerationStructureBuildRangeInfoKHR::default()
                        .primitive_count(counts[i])];
                    result
                        .api
                        .cmd_build_acceleration_structures(cmd, &[info], &[&range]);
                    Self::barrier(ctx, cmd);
                }
            })?;
            let mut placements = Vec::with_capacity(scene.ray_instances.len());
            for instance in &scene.ray_instances {
                let blas = result.api.get_acceleration_structure_device_address(
                    &vk::AccelerationStructureDeviceAddressInfoKHR::default()
                        .acceleration_structure(result.structures[instance.mesh as usize].handle),
                );
                placements.push(vk::AccelerationStructureInstanceKHR {
                    transform: vk::TransformMatrixKHR {
                        matrix: instance.transform,
                    },
                    instance_custom_index_and_mask: vk::Packed24_8::new(instance.mesh, 255),
                    instance_shader_binding_table_record_offset_and_flags: vk::Packed24_8::new(
                        0,
                        vk::GeometryInstanceFlagsKHR::TRIANGLE_FACING_CULL_DISABLE.as_raw() as u8,
                    ),
                    acceleration_structure_reference: vk::AccelerationStructureReferenceKHR {
                        device_handle: blas,
                    },
                });
            }
            let instances = Buffer::new(
                ctx,
                (placements.len() * 64) as u64,
                vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR
                    | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
                true,
            )?;
            let bytes =
                std::slice::from_raw_parts(placements.as_ptr().cast::<u8>(), placements.len() * 64);
            instances.write(0, bytes)?;
            let geometry = vk::AccelerationStructureGeometryKHR::default()
                .geometry_type(vk::GeometryTypeKHR::INSTANCES)
                .geometry(vk::AccelerationStructureGeometryDataKHR {
                    instances: vk::AccelerationStructureGeometryInstancesDataKHR::default().data(
                        vk::DeviceOrHostAddressConstKHR {
                            device_address: address(&instances),
                        },
                    ),
                });
            let info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
                .ty(vk::AccelerationStructureTypeKHR::TOP_LEVEL)
                .flags(vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE)
                .geometries(std::slice::from_ref(&geometry));
            let count = placements.len() as u32;
            let mut size = vk::AccelerationStructureBuildSizesInfoKHR::default();
            result.api.get_acceleration_structure_build_sizes(
                vk::AccelerationStructureBuildTypeKHR::DEVICE,
                &info,
                &[count],
                &mut size,
            );
            result.structures.push(result.allocate(
                size.acceleration_structure_size,
                vk::AccelerationStructureTypeKHR::TOP_LEVEL,
            )?);
            let scratch = Buffer::new(
                ctx,
                size.build_scratch_size + alignment,
                vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
                false,
            )?;
            let info = info
                .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
                .dst_acceleration_structure(result.handle())
                .scratch_data(vk::DeviceOrHostAddressKHR {
                    device_address: address(&scratch).div_ceil(alignment) * alignment,
                });
            immediate(ctx, pool, |cmd| {
                result.api.cmd_build_acceleration_structures(
                    cmd,
                    &[info],
                    &[&[vk::AccelerationStructureBuildRangeInfoKHR::default()
                        .primitive_count(count)]],
                );
                let barrier = vk::MemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_KHR)
                    .dst_access_mask(vk::AccessFlags::ACCELERATION_STRUCTURE_READ_KHR);
                ctx.device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
                    vk::PipelineStageFlags::FRAGMENT_SHADER
                        | vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::DependencyFlags::empty(),
                    &[barrier],
                    &[],
                    &[],
                );
            })?;
            println!(
                "RTX acceleration structures: {} BLAS, {} instances, {:.0} ms",
                geometries.len(),
                placements.len(),
                start.elapsed().as_secs_f64() * 1000.
            );
        }
        Ok(result)
    }
    unsafe fn allocate(
        &self,
        size: u64,
        ty: vk::AccelerationStructureTypeKHR,
    ) -> Result<Acceleration> {
        unsafe {
            let storage = Buffer::new(
                &self.ctx,
                size,
                vk::BufferUsageFlags::ACCELERATION_STRUCTURE_STORAGE_KHR
                    | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
                false,
            )?;
            let handle = self.api.create_acceleration_structure(
                &vk::AccelerationStructureCreateInfoKHR::default()
                    .buffer(storage.handle)
                    .size(size)
                    .ty(ty),
                None,
            )?;
            Ok(Acceleration {
                handle,
                _storage: storage,
            })
        }
    }
    unsafe fn barrier(ctx: &Context, cmd: vk::CommandBuffer) {
        unsafe {
            let b = vk::MemoryBarrier::default()
                .src_access_mask(
                    vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_KHR
                        | vk::AccessFlags::ACCELERATION_STRUCTURE_READ_KHR,
                )
                .dst_access_mask(
                    vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_KHR
                        | vk::AccessFlags::ACCELERATION_STRUCTURE_READ_KHR,
                );
            ctx.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
                vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
                vk::DependencyFlags::empty(),
                &[b],
                &[],
                &[],
            );
        }
    }
}
impl Drop for RayScene {
    fn drop(&mut self) {
        unsafe {
            for a in self.structures.iter().rev() {
                self.api.destroy_acceleration_structure(a.handle, None);
            }
        }
    }
}
