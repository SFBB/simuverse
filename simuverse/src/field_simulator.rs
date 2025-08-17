use crate::node::{BindGroupData, BufferlessFullscreenNode, ComputeNode};
use crate::util::BufferObj;
use crate::{FieldUniform, SettingObj, Simulator};
use alloc::vec;
use app_surface::AppSurface;
use wgpu::CommandEncoderDescriptor;

use crate::{create_shader_module, insert_code_then_create};

pub struct FieldSimulator {
    field_uniform: BufferObj,
    field_buf: BufferObj,
    field_workgroup_count: (u32, u32, u32),
    _trajectory_update_shader: wgpu::ShaderModule,
    field_setting_node: ComputeNode,
    particles_update_node: ComputeNode,
    render_node: BufferlessFullscreenNode,
    frame_num: usize,
}

impl FieldSimulator {
    pub fn new(
        app: &app_surface::AppSurface,
        canvas_format: wgpu::TextureFormat,
        canvas_size: glam::UVec2,
        canvas_buf: &BufferObj,
        setting: &SettingObj,
    ) -> Self {
        let pixel_distance = 4;
        let field_size = glam::UVec2::new(
            canvas_size.x / pixel_distance,
            canvas_size.y / pixel_distance,
        );

        let field_workgroup_count = (field_size.x.div_ceil(16), field_size.y.div_ceil(16), 1);
        let (_, sx, sy) = crate::util::matrix_helper::fullscreen_factor(
            (canvas_size.x as f32, canvas_size.y as f32).into(),
            75.0 / 180.0 * core::f32::consts::PI,
        );

        let field_uniform_data = FieldUniform {
            lattice_size: [field_size.x as i32, field_size.y as i32],
            lattice_pixel_size: [pixel_distance as f32; 2],
            canvas_size: [canvas_size.x as i32, canvas_size.y as i32],
            proj_ratio: [sx, sy],
            ndc_pixel: [
                sx * 2.0 / canvas_size.x as f32,
                sy * 2.0 / canvas_size.y as f32,
            ],
            speed_ty: 0,
            _padding: 0.0,
        };
        let field_uniform = BufferObj::create_uniform_buffer(
            &app.device,
            &field_uniform_data,
            Some("field_uniform"),
        );
        let field_buf = BufferObj::create_empty_storage_buffer(
            &app.device,
            (field_size.x * field_size.y * 16) as u64,
            false,
            Some("field buf"),
        );

        let code_snippet = crate::get_velocity_code_snippet(setting.animation_type);
        let setting_shader =
            insert_code_then_create(&app.device, "field_setting", Some(&code_snippet), None);

        let field_setting_node = ComputeNode::new(
            &app.device,
            &BindGroupData {
                workgroup_count: field_workgroup_count,
                uniforms: vec![&field_uniform],
                storage_buffers: vec![&field_buf],
                ..Default::default()
            },
            &setting_shader,
        );

        let trajectory_update_shader = create_shader_module(&app.device, "trajectory_update", None);
        let particles_update_node = ComputeNode::new(
            &app.device,
            &BindGroupData {
                workgroup_count: setting.particles_workgroup_count,
                uniforms: vec![&field_uniform, setting.particles_uniform.as_ref().unwrap()],
                storage_buffers: vec![
                    &field_buf,
                    setting.particles_buf.as_ref().unwrap(),
                    canvas_buf,
                ],
                ..Default::default()
            },
            &trajectory_update_shader,
        );

        let render_shader = create_shader_module(&app.device, "present", None);
        let render_node = BufferlessFullscreenNode::new(
            &app.device,
            canvas_format,
            &BindGroupData {
                uniforms: vec![&field_uniform, setting.particles_uniform.as_ref().unwrap()],
                storage_buffers: vec![canvas_buf],
                ..Default::default()
            },
            &render_shader,
            None,
        );

        let mut instance = FieldSimulator {
            field_uniform,
            field_buf,
            field_workgroup_count,
            _trajectory_update_shader: trajectory_update_shader,
            field_setting_node,
            particles_update_node,
            render_node,
            frame_num: 0,
        };

        instance.reset(app);
        instance
    }

    pub fn update_field_by_cpass<'c, 'b: 'c>(&'b self, cpass: &mut wgpu::ComputePass<'c>) {
        self.field_setting_node.compute_by_pass(cpass);
    }
}

impl Simulator for FieldSimulator {
    fn reset(&mut self, app: &app_surface::AppSurface) {
        let mut encoder = app
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("update_field encoder"),
            });
        self.field_setting_node.compute(&mut encoder);
        app.queue.submit(Some(encoder.finish()));
    }

    fn update_by(&mut self, app: &AppSurface, control_panel: &mut crate::ControlPanel) {
        if !control_panel.is_code_snippet_changed() {
            return;
        }

        let setting_shader = insert_code_then_create(
            &app.device,
            "field_setting",
            Some(&control_panel.wgsl_code),
            None,
        );

        self.field_setting_node = ComputeNode::new(
            &app.device,
            &BindGroupData {
                workgroup_count: self.field_workgroup_count,
                uniforms: vec![&self.field_uniform],
                storage_buffers: vec![&self.field_buf],
                ..Default::default()
            },
            &setting_shader,
        );
        self.reset(app);
    }

    fn update_workgroup_count(
        &mut self,
        _app: &app_surface::AppSurface,
        workgroup_count: (u32, u32, u32),
    ) {
        self.particles_update_node.workgroup_count = workgroup_count;
    }

    fn compute(&mut self, encoder: &mut wgpu::CommandEncoder) {
        self.particles_update_node.compute(encoder);
    }

    fn draw_by_rpass<'b, 'a: 'b>(
        &'a mut self,
        _app: &app_surface::AppSurface,
        rpass: &mut wgpu::RenderPass<'b>,
        _setting: &mut crate::SettingObj,
    ) {
        self.render_node.draw_by_pass(rpass);
        self.frame_num += 1;
    }
}
