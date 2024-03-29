mod context;
mod lobby_stage;
pub mod logging;
mod message;
mod play_stage;
mod replay_stage;
pub mod sync_manager;
mod sync_stage;

use std::path::PathBuf;

use godot::{
    engine::{EditorPlugin, IEditorPlugin, ProjectSettings, ResourceSaver},
    prelude::*,
};

pub use context::Context;
pub use message::SentInput;
use sync_manager::RollbackSyncManager;

struct GdRollback {}

#[gdextension]
unsafe impl ExtensionLibrary for GdRollback {}

#[derive(GodotClass)]
#[class(tool, editor_plugin, base=EditorPlugin)]
struct GdRollbackEditorPlugin {
    base: Base<EditorPlugin>,
}

#[godot_api]
impl IEditorPlugin for GdRollbackEditorPlugin {
    fn init(base: Base<EditorPlugin>) -> Self {
        GdRollbackEditorPlugin { base }
    }

    fn enter_tree(&mut self) {
        let project_settings = ProjectSettings::singleton();
        let directory_string: String = project_settings
            .globalize_path("res://autoloads".into())
            .into();
        let directory_path = PathBuf::from(directory_string);

        std::fs::create_dir_all(directory_path).expect("Couldn't create autoloads directory");

        let autoloads: Vec<(GString, GString, Gd<Node>)> = vec![(
            "SyncManager".into(),
            "res://autoloads/sync_manager.tscn".into(),
            RollbackSyncManager::new_alloc().upcast::<Node>(),
        )];

        for (name, path, instance) in autoloads.into_iter() {
            let mut resource_saver = ResourceSaver::singleton();
            let mut packed_scene = PackedScene::new_gd();
            packed_scene.pack(instance);
            resource_saver
                .save_ex(packed_scene.upcast())
                .path(path.clone())
                .done();
            self.base_mut().add_autoload_singleton(name, path);
        }
    }
}
