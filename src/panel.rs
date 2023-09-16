use map_range::MapRange;
use mint::Vector2;
use rustc_hash::FxHashMap;
use stardust_xr_fusion::{
	client::FrameInfo,
	core::values::Transform,
	drawable::{MaterialParameter, Model, ResourceID},
	fields::BoxField,
	items::{
		panel::{ChildInfo, Geometry, PanelItem, PanelItemHandler, PanelItemInitData, SurfaceID},
		ItemUI, ItemUIHandler,
	},
	node::{NodeError, NodeType},
	HandlerWrapper,
};
use stardust_xr_molecules::{Grabbable, GrabbableSettings};
use std::sync::{Arc, Mutex};

pub struct PanelItemUIHandler {
	item_ui: ItemUI<PanelItem>,
	items: FxHashMap<String, HandlerWrapper<PanelItem, PanelItemUI>>,
}
impl PanelItemUIHandler {
	pub fn new(item_ui: ItemUI<PanelItem>) -> Self {
		PanelItemUIHandler {
			item_ui,
			items: FxHashMap::default(),
		}
	}
	pub fn frame(&mut self, info: &FrameInfo) {
		for (_, item) in self.items.iter() {
			item.lock_wrapped().frame(info, &self.item_ui);
		}
	}
}
impl ItemUIHandler<PanelItem> for PanelItemUIHandler {
	fn item_created(&mut self, uid: &str, item: PanelItem, init_data: PanelItemInitData) {
		let Ok(ui) = PanelItemUI::new(item.alias(), init_data) else {return};
		let Ok(ui) = item.wrap(ui) else {return};
		self.items.insert(uid.to_string(), ui);
	}
	fn item_captured(&mut self, uid: &str, acceptor_uid: &str, _item: PanelItem) {
		if let Some(ui) = self.items.get(uid) {
			ui.lock_wrapped().captured(acceptor_uid);
		}
	}
	fn item_released(&mut self, uid: &str, acceptor_uid: &str, _item: PanelItem) {
		if let Some(ui) = self.items.get(uid) {
			ui.lock_wrapped().released(acceptor_uid);
		}
	}
	fn item_destroyed(&mut self, uid: &str) {
		self.items.remove(uid);
	}
}

const PANEL_WIDTH: f32 = 0.1;
const PANEL_THICKNESS: f32 = 0.01;
const MAX_ACCEPT_DISTANCE: f32 = 0.05;
struct PanelItemUI {
	captured: bool,
	panel_item: PanelItem,
	model: Model,
	field: BoxField,
	grabbable: Grabbable,
	// update_position_task: JoinHandle<()>,
}
impl PanelItemUI {
	fn new(panel_item: PanelItem, init_data: PanelItemInitData) -> Result<Self, NodeError> {
		let field = BoxField::create(
			&panel_item,
			Transform::default(),
			[PANEL_WIDTH, PANEL_WIDTH, PANEL_THICKNESS],
		)?;
		let grabbable = Grabbable::create(
			&panel_item,
			Transform::default(),
			&field,
			GrabbableSettings::default(),
		)?;
		let model = Model::create(
			&panel_item,
			Transform::from_scale([PANEL_WIDTH, PANEL_WIDTH, PANEL_THICKNESS]),
			&ResourceID::new_namespaced("orbit", "panel"),
		)?;

		panel_item.auto_size_toplevel()?;
		panel_item.apply_surface_material(&SurfaceID::Toplevel, &model.model_part("Face")?)?;
		panel_item.set_spatial_parent_in_place(grabbable.content_parent())?;

		let closest_acceptor_distance = Arc::new(Mutex::new((String::new(), f32::MAX)));
		let _closest_acceptor_distance = closest_acceptor_distance.clone();

		let mut panel_item_ui = PanelItemUI {
			captured: false,
			panel_item,
			model,
			field,
			grabbable,
			// update_position_task,
		};
		panel_item_ui.on_resize(init_data.toplevel.size);
		Ok(panel_item_ui)
	}
	fn captured(&mut self, _acceptor_uid: &str) {
		println!("Captured");
		self.update_state(true);
		self.grabbable.cancel_linear_velocity();
		self.grabbable.cancel_angular_velocity();
	}
	fn released(&mut self, _acceptor_uid: &str) {
		println!("Released");
		self.update_state(false);
		let _ = self
			.grabbable
			.content_parent()
			.set_transform(Some(&self.panel_item), Transform::identity());
		let _ = self.panel_item.set_transform(None, Transform::identity());
	}
	fn update_state(&mut self, captured: bool) {
		self.captured = captured;
		let _ = self.model.set_enabled(!captured);
		let _ = self.grabbable.set_enabled(!captured);
	}
	fn frame(&mut self, info: &FrameInfo, item_ui: &ItemUI<PanelItem>) {
		if self.captured {
			return;
		}
		self.grabbable.update(info).unwrap();
		self.update_distances(
			item_ui,
			!self.grabbable.grab_action().actor_acting() && self.grabbable.linear_speed().is_some()
				|| self.grabbable.grab_action().actor_stopped(),
		);
	}

	fn update_distances(&self, item_ui: &ItemUI<PanelItem>, accept: bool) {
		if self.captured {
			return;
		}
		let acceptors = item_ui.acceptors();
		if acceptors.is_empty() {
			return;
		}
		let keys = acceptors.keys().cloned().collect::<Vec<String>>();
		let fields = acceptors
			.values()
			.map(|(_, f)| f.alias())
			.collect::<Vec<_>>();

		let Ok(future) = self.panel_item
            .field_distance([0.0; 3], fields) else {return};
		let model = self.model.alias();
		let item_ui = item_ui.alias();
		let panel_item = self.panel_item.alias();
		tokio::spawn(async move {
			let Ok(distances) = future.await else {return};
			// dbg!(&distances);
			let Some((uid, distance)) = keys.into_iter()
                .zip(distances.into_iter().map(|d| d.map(|d| d.abs())))
                .filter_map(|(k, v)| Some((k, v?)))
                .reduce(|(ak, av), (bk, bv)| {
                    if av > bv {
                        (bk, bv)
                    } else {
                        (ak, av)
                    }
                }) else {
					let _ = model.model_part("Edge").unwrap().set_material_parameter("color", MaterialParameter::Color([1.0; 4]));
					return;
				};

			let gradient = colorgrad::magma();
			let color = gradient.at(distance.map_range(0.25..MAX_ACCEPT_DISTANCE, 0.0..1.0) as f64);
			let _ = model.model_part("Edge").unwrap().set_material_parameter(
				"color",
				MaterialParameter::Color(color.to_array().map(|c| c as f32)),
			);
			if accept && distance < MAX_ACCEPT_DISTANCE {
				let acceptors = item_ui.acceptors();
				let Some(acceptor) = acceptors.get(&uid).map(|(a, _)| a) else {return};
				let _ = acceptor.capture(&panel_item);
			}
		});
	}

	fn on_resize(&mut self, size: Vector2<u32>) {
		let aspect_ratio = size.y as f32 / size.x as f32;
		let size = [PANEL_WIDTH, PANEL_WIDTH * aspect_ratio, PANEL_THICKNESS];
		let _ = self.model.set_scale(None, size);
		let _ = self.field.set_size(size);
	}
}
impl PanelItemHandler for PanelItemUI {
	fn toplevel_size_changed(&mut self, size: mint::Vector2<u32>) {
		self.on_resize(size);
	}

	fn new_child(&mut self, _uid: &str, _info: ChildInfo) {}
	fn reposition_child(&mut self, _uid: &str, _geometry: Geometry) {}
	fn drop_child(&mut self, _uid: &str) {}
}
impl Drop for PanelItemUI {
	fn drop(&mut self) {
		// self.update_position_task.abort();
	}
}
