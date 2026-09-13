// utilities
use crate::colors;

// enums
use crate::enums;
use enums::{ // CLI main menu options
    command_options, 
    manifests_options, 
    Commands,
    SteamUploaderConfig,
};

// LIBRARIES
use confy; // for configuration management
use inquire; // for interactive CLI menu







// INTERACTIVE MENU
// used when no command is provided via CLI arguments
// shows a menu to select the command from
pub fn display_main_menu() -> Commands {
    let options = vec![
        command_options::MANAGE_CONFIG, // first for quick access
        command_options::INIT,
        command_options::UPLOAD,
        command_options::DELETE,
        command_options::EXIT,
    ];

    let selection = inquire::Select::new("What would you like to do?", options.clone())
        .prompt()
        .expect("Failed to read selection");

    match selection {
        // initialize a new manifest file
        command_options::INIT => {
            // ask for the format of the manifest file
            let format = inquire::Select::new(
                "Choose manifest format:",
                vec!["json", "toml", "yaml"],
            )
            .prompt()
            .expect("Failed to read format");

            // run
            return Commands::Init {
                format: format.to_string(),
            };
        }


        // upload content to steam workshop
        command_options::UPLOAD => {
            return display_upload_menu( None );
        }


        // delete a workshop item
        command_options::DELETE => {
            // ask for the workshop ID to delete
            let workshopid = inquire::Text::new("Workshop ID to delete:")
                .prompt()
                .expect("Failed to read input")
                .parse::<u64>()
                .expect("Invalid Workshop ID");

            // ask for the app ID
            let appid = inquire::Text::new("App ID:")
                .prompt()
                .expect("Failed to read input")
                .parse::<u32>()
                .expect("Invalid App ID");

            // run
            return Commands::Delete { workshopid, appid };
        }

        // manage manifest configurations
        command_options::MANAGE_CONFIG => {
            return display_manifests_menu();
        }

        // exit the program
        command_options::EXIT => {
            std::process::exit(0);
        }

        // this should never happen since we control the options, but just in case
        _ => unreachable!(),
    }
}



fn display_upload_menu( mut manifest_path: Option<String> ) -> Commands {
    if manifest_path.is_none() {
        // ask for custom manifest file path
        manifest_path = inquire::Text::new("Manifest file path (leave empty for default):")
            .prompt()
            .ok()
            .and_then(|p| if p.is_empty() { None } else { Some(p) });
    }

    // ask for patch note file path (optional)
    let patchnote = inquire::Text::new("Patch note file path (optional, leave empty to skip):")
        .prompt()
        .ok()
        .and_then(|p| if p.is_empty() { None } else { Some(p) });

    // ask if they want to do a dry run (default: no)
    let dry_run = inquire::Confirm::new("Perform a dry run (no actual upload)?")
        .with_default(false)
        .prompt()
        .unwrap_or(false);

    return Commands::Upload {
        patchnote,
        manifest_path,
        dry_run,
    };
}



// MANAGE CONFIGURATION
// allows users to create, view, add, and remove manifest configurations
pub fn display_manifests_menu() -> Commands {
    loop {
        // load config (creates empty if doesn't exist)
        let config: SteamUploaderConfig = confy::load("steam-uploader", None)
            .expect("Failed to load configuration");

        // access available manifests
        let manifests = config.manifests.clone();
        let mut manifest_indices = vec![];

        // populate options with existing manifests for quick access
        let mut options = vec![];
        for (i, manifest) in manifests.iter().enumerate() {
            let option = format!("{} - {} ({})", i, manifest.name, manifest.path);
            options.push(option.clone());
            manifest_indices.push(i);
        }

        // add options to add and remove manifests at the end of the list
        options.push(manifests_options::ADD_MANIFEST.to_string());
        options.push(manifests_options::REMOVE_MANIFEST.to_string());
        options.push(manifests_options::BACK.to_string());


        // chose manifest or add/remove
        let selection = inquire::Select::new(
            "Select a manifest to upload",
            options.clone())
            .prompt()
            .expect("Failed to read selection");

        match selection.as_str() {
            manifests_options::ADD_MANIFEST => {
                let name = inquire::Text::new("Manifest name (e.g., 'Main Mod'):")
                    .prompt()
                    .expect("Failed to read manifest name");

                let path = inquire::Text::new("Manifest file path:")
                    .prompt()
                    .expect("Failed to read manifest file path");

                return Commands::AddManifest { name, path };
            }

            manifests_options::REMOVE_MANIFEST => {
                if config.manifests.is_empty() {
                    colors::warning("No manifests in config to remove.");
                    continue;
                }

                let names: Vec<String> = config.manifests.iter().map(|m| m.name.clone()).collect();
                let name = inquire::Select::new("Select manifest to remove:", names)
                    .prompt()
                    .expect("Failed to read selection");

                return Commands::RemoveManifest { name };
            }

            manifests_options::BACK => {
                return display_main_menu();
            }

            _ => {
                // Find which manifest was selected
                if let Some(pos) = options.iter().position(|o| o == &selection) {
                    if pos < manifest_indices.len() {
                        let index = manifest_indices[pos];
                        let selected = &manifests[index];
                        colors::info(&format!("Selected manifest: {} ({})", selected.name, selected.path));

                        let manifest_path = Some(selected.path.clone());
                        return display_upload_menu(manifest_path);
                    }
                }
            }
        }
    }
}
