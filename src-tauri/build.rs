fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "refresh_network",
            "cached_devices",
            "update_device",
            "get_settings",
            "save_setting",
            "list_tasks",
            "task_detail",
            "submit_repair",
            "app_info",
        ]),
    ))
    .expect("failed to build Tauri resources");
}
