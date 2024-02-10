pub mod panel;

use std::sync::Arc;

use color_eyre::eyre::Result;
use manifest_dir_macros::directory_relative_path;
use panel::PanelItemUIHandler;
use stardust_xr_fusion::{
	client::{Client, ClientState, FrameInfo, RootHandler},
	items::{panel::PanelItem, ItemUI},
	HandlerWrapper,
};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
	color_eyre::install()?;
	let (client, event_loop) = Client::connect_with_async_loop().await?;
	client.set_base_prefixes(&[directory_relative_path!("res")]);

	let _wrapped_root = client.wrap_root(Orbit::new(&client)?)?;

	tokio::select! {
		_ = tokio::signal::ctrl_c() => (),
		e = event_loop => e??,
	}
	Ok(())
}

struct Orbit {
	panel_item_ui: HandlerWrapper<ItemUI<PanelItem>, PanelItemUIHandler>,
}
impl Orbit {
	fn new(client: &Arc<Client>) -> Result<Self> {
		let panel_item_ui = ItemUI::register(client)?;
		let panel_item_ui_handler = PanelItemUIHandler::new();
		Ok(Orbit {
			panel_item_ui: panel_item_ui.wrap(panel_item_ui_handler)?,
		})
	}
}
impl RootHandler for Orbit {
	fn frame(&mut self, info: FrameInfo) {
		self.panel_item_ui.lock_wrapped().frame(&info);
	}

	fn save_state(&mut self) -> ClientState {
		ClientState::default()
	}
}
