use tauri::{
    AppHandle, Result,
    menu::{MenuItem, MenuItemKind},
};

use super::MenuItemHandler;

pub struct TrayVersion;

impl TrayVersion {
    // Der Kanal wird zuerst an der Kennung abgelesen und nur ersatzweise am
    // Anzeigenamen. Vorher stand hier ausschliesslich die Upstream-Kennung,
    // die keiner unserer Bauten traegt -- der Kanal haing damit allein am
    // Anzeigenamen "Anarlog". Mit dem Umbenennen auf "Mitschnitt" waere jeder
    // Release-Bau stillschweigend als "dev" im Tray-Menue gelandet.
    //
    // Die Altnamen bleiben als Ersatzpfad stehen: eine bereits installierte
    // Kopie meldet sich noch unter ihnen.
    fn get_channel(identifier: &str, app_name: &str) -> &'static str {
        match identifier {
            "media.zickert.mitschnitt.stable"
            | "media.zickert.mitschnitt.flatpak"
            | "media.zickert.mitschnitt.desktop"
            | "com.hyprnote.stable"
            | "com.hyprnote.Hyprnote" => "stable",
            "media.zickert.mitschnitt.staging" | "com.hyprnote.staging" => "staging",
            "media.zickert.mitschnitt" | "com.hyprnote.dev" => "dev",
            _ => match app_name {
                "Mitschnitt" | "Anarlog" | "Char" | "Hyprnote" => "stable",
                "Mitschnitt Staging" | "Anarlog Staging" | "Char Staging" | "Hyprnote Staging" => {
                    "staging"
                }
                _ => "dev",
            },
        }
    }
}

impl MenuItemHandler for TrayVersion {
    const ID: &'static str = "anlg_tray_version";

    fn build(app: &AppHandle<tauri::Wry>) -> Result<MenuItemKind<tauri::Wry>> {
        let identifier = &app.config().identifier;
        let app_name = &app.package_info().name;
        let app_version = app.package_info().version.to_string();
        let channel = Self::get_channel(identifier, app_name);

        let text = format!("v{} ({})", app_version, channel);
        let item = MenuItem::with_id(app, Self::ID, text, false, None::<&str>)?;
        Ok(MenuItemKind::MenuItem(item))
    }

    fn handle(_app: &AppHandle<tauri::Wry>) {}
}

#[cfg(test)]
mod tests {
    use super::TrayVersion;

    #[test]
    fn gets_channel_from_identifier() {
        assert_eq!(
            TrayVersion::get_channel("com.hyprnote.stable", "Anarlog"),
            "stable"
        );
        assert_eq!(
            TrayVersion::get_channel("com.hyprnote.staging", "Anarlog Staging"),
            "staging"
        );
        assert_eq!(
            TrayVersion::get_channel("com.hyprnote.dev", "Anarlog Dev"),
            "dev"
        );
    }

    // Die Paare stammen 1:1 aus apps/desktop/src-tauri/tauri.conf.*.json.
    // Genau diese Kombinationen laufen in einem echten Bau durch die
    // Funktion; keine davon darf auf "dev" fallen, ausser der Dev-Bau selbst.
    #[test]
    fn ships_the_right_channel_for_every_real_build() {
        for (identifier, product_name, expected) in [
            ("media.zickert.mitschnitt.stable", "Mitschnitt", "stable"),
            ("media.zickert.mitschnitt.flatpak", "Mitschnitt", "stable"),
            ("media.zickert.mitschnitt.desktop", "Mitschnitt", "stable"),
            (
                "media.zickert.mitschnitt.staging",
                "Mitschnitt Staging",
                "staging",
            ),
            ("media.zickert.mitschnitt", "Mitschnitt", "dev"),
        ] {
            assert_eq!(
                TrayVersion::get_channel(identifier, product_name),
                expected,
                "{identifier} / {product_name} meldet den falschen Kanal"
            );
        }
    }

    // Der Anzeigename allein muss reichen, falls die Kennung einmal nicht
    // passt -- sonst haengt alles an einer einzigen Liste.
    #[test]
    fn falls_back_to_the_new_product_name() {
        assert_eq!(TrayVersion::get_channel("unknown", "Mitschnitt"), "stable");
        assert_eq!(
            TrayVersion::get_channel("unknown", "Mitschnitt Staging"),
            "staging"
        );
    }

    #[test]
    fn falls_back_to_product_name_for_unknown_identifier() {
        assert_eq!(TrayVersion::get_channel("unknown", "Anarlog"), "stable");
        assert_eq!(
            TrayVersion::get_channel("unknown", "Anarlog Staging"),
            "staging"
        );
        assert_eq!(TrayVersion::get_channel("unknown", "Anarlog Dev"), "dev");
    }
}
