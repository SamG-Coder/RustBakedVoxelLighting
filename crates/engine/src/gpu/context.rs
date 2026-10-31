use anyhow::{Result, bail};
use ash::{Device, Entry, Instance, vk};
use std::{
    ffi::{CStr, c_void},
    sync::Arc,
};
use winit::{
    raw_window_handle::{HasDisplayHandle, HasWindowHandle},
    window::Window,
};

pub struct Context {
    pub _entry: Entry,
    pub instance: Instance,
    pub device: Device,
    pub physical: vk::PhysicalDevice,
    pub queue: vk::Queue,
    pub family: u32,
    pub surface_api: ash::khr::surface::Instance,
    pub surface: vk::SurfaceKHR,
    pub memory: vk::PhysicalDeviceMemoryProperties,
    pub ray_query: bool,
    debug: Option<(ash::ext::debug_utils::Instance, vk::DebugUtilsMessengerEXT)>,
}
unsafe extern "system" fn debug_message(
    _: vk::DebugUtilsMessageSeverityFlagsEXT,
    _: vk::DebugUtilsMessageTypeFlagsEXT,
    data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    _: *mut c_void,
) -> vk::Bool32 {
    if !data.is_null() {
        eprintln!(
            "VULKAN: {}",
            unsafe { CStr::from_ptr((*data).p_message) }.to_string_lossy()
        );
    }
    vk::FALSE
}
impl Context {
    pub fn new(window: &Window) -> Result<Arc<Self>> {
        unsafe {
            let entry = Entry::load()?;
            let mut extensions =
                ash_window::enumerate_required_extensions(window.display_handle()?.as_raw())?
                    .to_vec();
            let available = entry.enumerate_instance_layer_properties()?;
            let validation = std::env::var_os("BVL_VALIDATION").is_some()
                && available.iter().any(|p| {
                    CStr::from_ptr(p.layer_name.as_ptr()) == c"VK_LAYER_KHRONOS_validation"
                });
            let layers = if validation {
                vec![c"VK_LAYER_KHRONOS_validation".as_ptr()]
            } else {
                vec![]
            };
            if validation {
                extensions.push(ash::ext::debug_utils::NAME.as_ptr());
            }
            let app = vk::ApplicationInfo::default()
                .application_name(c"RustBakedVoxelLighting")
                .engine_name(c"BVL Vulkan")
                .api_version(vk::API_VERSION_1_3);
            let instance = entry.create_instance(
                &vk::InstanceCreateInfo::default()
                    .application_info(&app)
                    .enabled_extension_names(&extensions)
                    .enabled_layer_names(&layers),
                None,
            )?;
            let debug = if validation {
                let api = ash::ext::debug_utils::Instance::new(&entry, &instance);
                let messenger = api.create_debug_utils_messenger(
                    &vk::DebugUtilsMessengerCreateInfoEXT::default()
                        .message_severity(
                            vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                                | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING,
                        )
                        .message_type(
                            vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                                | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                                | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
                        )
                        .pfn_user_callback(Some(debug_message)),
                    None,
                )?;
                Some((api, messenger))
            } else {
                None
            };
            let surface = ash_window::create_surface(
                &entry,
                &instance,
                window.display_handle()?.as_raw(),
                window.window_handle()?.as_raw(),
                None,
            )?;
            let surface_api = ash::khr::surface::Instance::new(&entry, &instance);
            let mut candidates = Vec::new();
            for physical in instance.enumerate_physical_devices()? {
                let props = instance.get_physical_device_properties(physical);
                if props.api_version < vk::API_VERSION_1_3 {
                    continue;
                }
                for (i, q) in instance
                    .get_physical_device_queue_family_properties(physical)
                    .iter()
                    .enumerate()
                {
                    if q.queue_flags
                        .contains(vk::QueueFlags::GRAPHICS | vk::QueueFlags::COMPUTE)
                        && surface_api
                            .get_physical_device_surface_support(physical, i as u32, surface)?
                    {
                        candidates.push((
                            props.device_type == vk::PhysicalDeviceType::DISCRETE_GPU,
                            physical,
                            i as u32,
                            props,
                        ));
                        break;
                    }
                }
            }
            candidates.sort_by_key(|c| !c.0);
            let Some((_, physical, family, props)) = candidates.first().copied() else {
                bail!("No Vulkan 1.3 graphics/present adapter");
            };
            let supported = instance.get_physical_device_features(physical);
            anyhow::ensure!(
                supported.shader_sampled_image_array_dynamic_indexing == vk::TRUE,
                "Adapter lacks sampled image array indexing"
            );
            let mut supported13 = vk::PhysicalDeviceVulkan13Features::default();
            instance.get_physical_device_features2(
                physical,
                &mut vk::PhysicalDeviceFeatures2::default().push_next(&mut supported13),
            );
            anyhow::ensure!(
                supported13.dynamic_rendering == vk::TRUE,
                "Dynamic rendering unavailable"
            );
            let extension_list = instance.enumerate_device_extension_properties(physical)?;
            let has = |name: &CStr| {
                extension_list
                    .iter()
                    .any(|e| CStr::from_ptr(e.extension_name.as_ptr()) == name)
            };
            let mut f12_supported = vk::PhysicalDeviceVulkan12Features::default();
            let mut as_supported = vk::PhysicalDeviceAccelerationStructureFeaturesKHR::default();
            let mut rq_supported = vk::PhysicalDeviceRayQueryFeaturesKHR::default();
            instance.get_physical_device_features2(
                physical,
                &mut vk::PhysicalDeviceFeatures2::default()
                    .push_next(&mut f12_supported)
                    .push_next(&mut as_supported)
                    .push_next(&mut rq_supported),
            );
            let ray_query = std::env::var_os("BVL_DISABLE_RT").is_none()
                && has(ash::khr::acceleration_structure::NAME)
                && has(ash::khr::ray_query::NAME)
                && has(ash::khr::deferred_host_operations::NAME)
                && as_supported.acceleration_structure != 0
                && rq_supported.ray_query != 0
                && f12_supported.buffer_device_address != 0
                && f12_supported.shader_sampled_image_array_non_uniform_indexing != 0;
            println!(
                "RTX capability: ray queries={ray_query}, ray tracing pipeline={}, mesh shaders={}, shader execution reorder={}",
                has(ash::khr::ray_tracing_pipeline::NAME),
                has(ash::ext::mesh_shader::NAME),
                has(c"VK_NV_ray_tracing_invocation_reorder")
            );
            let priorities = [1.];
            let queues = [vk::DeviceQueueCreateInfo::default()
                .queue_family_index(family)
                .queue_priorities(&priorities)];
            let mut ext = vec![ash::khr::swapchain::NAME.as_ptr()];
            if ray_query {
                ext.extend([
                    ash::khr::acceleration_structure::NAME.as_ptr(),
                    ash::khr::ray_query::NAME.as_ptr(),
                    ash::khr::deferred_host_operations::NAME.as_ptr(),
                ]);
            }
            let features = vk::PhysicalDeviceFeatures::default()
                .sampler_anisotropy(supported.sampler_anisotropy != 0)
                .shader_sampled_image_array_dynamic_indexing(true);
            anyhow::ensure!(
                supported13.shader_demote_to_helper_invocation == vk::TRUE,
                "Shader demote unavailable"
            );
            let mut f13 = vk::PhysicalDeviceVulkan13Features::default()
                .dynamic_rendering(true)
                .shader_demote_to_helper_invocation(true);
            let mut f12 = vk::PhysicalDeviceVulkan12Features::default()
                .buffer_device_address(ray_query)
                .shader_sampled_image_array_non_uniform_indexing(ray_query);
            let mut as_features = vk::PhysicalDeviceAccelerationStructureFeaturesKHR::default()
                .acceleration_structure(ray_query);
            let mut rq_features =
                vk::PhysicalDeviceRayQueryFeaturesKHR::default().ray_query(ray_query);
            let mut info = vk::DeviceCreateInfo::default()
                .queue_create_infos(&queues)
                .enabled_extension_names(&ext)
                .enabled_features(&features)
                .push_next(&mut f13)
                .push_next(&mut f12);
            if ray_query {
                info = info.push_next(&mut as_features).push_next(&mut rq_features);
            }
            let device = instance.create_device(physical, &info, None)?;
            let queue = device.get_device_queue(family, 0);
            let memory = instance.get_physical_device_memory_properties(physical);
            println!(
                "GPU: {} | direct Vulkan {}.{}.{} | validation={validation}",
                CStr::from_ptr(props.device_name.as_ptr()).to_string_lossy(),
                vk::api_version_major(props.api_version),
                vk::api_version_minor(props.api_version),
                vk::api_version_patch(props.api_version)
            );
            Ok(Arc::new(Self {
                _entry: entry,
                instance,
                device,
                physical,
                queue,
                family,
                surface_api,
                surface,
                memory,
                ray_query,
                debug,
            }))
        }
    }
    pub fn memory_type(&self, bits: u32, flags: vk::MemoryPropertyFlags) -> Result<u32> {
        for i in 0..self.memory.memory_type_count {
            if bits & (1 << i) != 0
                && self.memory.memory_types[i as usize]
                    .property_flags
                    .contains(flags)
            {
                return Ok(i);
            }
        }
        bail!("No compatible Vulkan memory type for {flags:?}")
    }
}
impl Drop for Context {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
            self.device.destroy_device(None);
            self.surface_api.destroy_surface(self.surface, None);
            if let Some((api, messenger)) = &self.debug {
                api.destroy_debug_utils_messenger(*messenger, None);
            }
            self.instance.destroy_instance(None);
        }
    }
}
