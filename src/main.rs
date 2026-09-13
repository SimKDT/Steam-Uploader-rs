// utilities
mod steam;
mod colors;
mod interact;

// enums
mod enums;
use enums::{
    Commands,
    ManifestConfigEntry,
    SteamUploaderConfig,
}; // CLI main menu options

// manifest handling
mod manifest;
use manifest::Manifest;

// LIBRARIES
use clap::{Parser}; // CLI argument parsing
use steamworks; // rust bindings for the Steamworks API




// PARSER INFORMATION AND CLI DEFINITIONS
#[derive(Parser)]
#[command(name = "Steam Uploader")]
#[command(about = "Upload mods to Steam Workshop", long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Option<Commands>,
}


// MAIN ENTRY POINT
fn main() {
    let args = Args::parse();
    
    // no command provided, show interactive menu
    if args.command.is_none() {
        // never end the menu loop, unless EXIT is selected which calls exit(0)
        loop {
            let command = interact::display_main_menu(); // get command
            execute_command(command); // run
        }
    }

    // run command by CLI argument
    let command = args.command.unwrap();
    execute_command(command);
}





// HANDLING OF THE SELECTED COMMAND
fn execute_command(command: Commands) {
    match command {
        // initialize a new manifest file with default values
        Commands::Init { format } => {
            Manifest::init(&format);
        }


        // handle the upload command
        Commands::Upload { patchnote, manifest_path, dry_run } => {
            match Manifest::load_default(manifest_path) {
                Ok(mut manifest) => {
                    let appid = manifest.appid;
                    let (client, ugc) = steam::load::load_steam(appid);

                    // verify that the workshop ID was provided
                    // if not, then we create a new item and update the manifest file
                    let published_id = if let Some(workshopid) = manifest.workshopid {
                        // already have a workshop ID, just upload
                        colors::info(&format!("Found existing workshopid in manifest: {}", workshopid));
                        steamworks::PublishedFileId(workshopid)
                    } else {
                        // no workshop ID, create one
                        colors::warning("No workshopid found in manifest. Creating new workshop item...");
                        match steam::create::create_item(&client, &ugc, manifest.appid, dry_run) {
                            Ok(id) => {
                                colors::success(&format!("Created workshop item: {:?}", id));
                                // update manifest with new workshopid and save it to the source file
                                if dry_run {
                                    colors::info("Dry run enabled. Skipping manifest update with new workshop ID.");
                                } else {
                                    match manifest.save_with_id_to_source(id.0) {
                                        Ok(_) => colors::info("Updated manifest with new workshopid"),
                                        Err(e) => colors::error(&format!("Warning: Could not update manifest file: {}", e)),
                                    }
                                }
                                id
                            }
                            Err(e) => {
                                colors::error(&format!("Error creating workshop item: {}", e));
                                return;
                            }
                        }
                    };

                    // resolve content and preview paths relative to the manifest file
                    let content_path = match manifest.get_content_path() {
                        Ok(path) => path.to_string_lossy().to_string(),
                        Err(e) => {
                            colors::error(&format!("Error resolving content path: {}", e));
                            return;
                        }
                    };
                    let preview_path = match manifest.get_preview_path() {
                        Ok(path) => path.to_string_lossy().to_string(),
                        Err(e) => {
                            colors::error(&format!("Error resolving preview path: {}", e));
                            return;
                        }
                    };
                    let description = match manifest.get_description() {
                        Ok(desc) => desc,
                        Err(e) => {
                            colors::error(&format!("Error reading description: {}", e));
                            return;
                        }
                    };

                    let patchnote = match manifest.try_read_file(patchnote.unwrap_or_default()) {
                        Ok(note) => Some(note),
                        Err(e) => {
                            colors::error(&format!("Error reading patch note: {}", e));
                            return;
                        }
                    };

                    // upload the content
                    colors::info("Uploading content to Steam Workshop...");
                    steam::uploader::upload_item_content(
                        &client,
                        &ugc,
                        manifest.appid,
                        published_id,
                        &content_path,
                        &preview_path,
                        &manifest.title,
                        &description,
                        manifest.visibility,
                        manifest.tags,
                        patchnote.as_deref(),
                        dry_run,
                    );
                }
                Err(e) => colors::error(&format!("Error loading manifest: {}", e)),
            }
        }


        // here we delete the workshop item provided by the user. Steam's API handles basically everything else,
        // we don't need to verify if it exists or if the user is the owner, etc.
        Commands::Delete { workshopid, appid } => {
            let (_, ugc) = steam::load::load_steam(appid);
            let published_id = steamworks::PublishedFileId(workshopid);
            steam::delete::delete_item(&ugc, published_id);
        }

        // add manifest configuration
        Commands::AddManifest { name, path } => {
            // load config (creates empty if doesn't exist)
            let mut config: SteamUploaderConfig = confy::load("steam-uploader", None)
                .expect("Failed to load configuration");

            // Add to vector
            config.manifests.push(ManifestConfigEntry { name: name.clone(), path });
            colors::success(&format!("Added '{}' to configuration", name));

            // Save immediately
            confy::store("steam-uploader", None, &config)
                .expect("Failed to save configuration");
            colors::success("Configuration saved");
        }

        // remove manifest configuration
        Commands::RemoveManifest { name } => {
            // load config (creates empty if doesn't exist)
            let mut config: SteamUploaderConfig = confy::load("steam-uploader", None)
                .expect("Failed to load configuration");

            if config.manifests.is_empty() {
                colors::warning("No manifests in config to remove.");
                return;
            }

            if let Some(index) = config.manifests.iter().position(|m| m.name == name) {
                config.manifests.remove(index);
                colors::success("Manifest removed from configuration.");
                
                // save immediately
                confy::store("steam-uploader", None, &config)
                    .expect("Failed to save configuration");
                colors::success("Configuration saved");
            }
        }
    }
}




// // MANAGE CONFIGURATION
// // allows users to create, view, add, and remove manifest configurations
// fn manage_config() -> Result<(), Box<dyn std::error::Error>> {
//     loop {
//         // load config (creates empty if doesn't exist)
//         let mut config: SteamUploaderConfig = confy::load("steam-uploader", None)?;

//         // access available manifests
//         let manifests = config.manifests.clone();
//         let mut manifest_indices = vec![];

//         // populate options with existing manifests for quick access
//         let mut options = vec![];
//         for (i, manifest) in manifests.iter().enumerate() {
//             let option = format!("{} - {} ({})", i, manifest.name, manifest.path);
//             options.push(option.clone());
//             manifest_indices.push(i);
//         }

//         // add options to add and remove manifests at the end of the list
//         options.push(manifests_options::ADD_MANIFEST.to_string());
//         options.push(manifests_options::REMOVE_MANIFEST.to_string());
//         options.push(manifests_options::BACK.to_string());

//         let selection = inquire::Select::new(
//             "Select a manifest to upload?",
//             options.clone())
//             .prompt()?;

        

//         match selection.as_str() {
//             manifests_options::ADD_MANIFEST => {
//                 let name = inquire::Text::new("Manifest name (e.g., 'Main Mod'):")
//                     .prompt()?;

//                 let path = inquire::Text::new("Manifest file path:")
//                     .prompt()?;

//                 // Add to vector
//                 config.manifests.push(ManifestConfigEntry { name: name.clone(), path });
//                 colors::success(&format!("Added '{}' to configuration", name));

//                 // Save immediately
//                 confy::store("steam-uploader", None, &config)?;
//                 colors::success("Configuration saved");
//             }

//             manifests_options::REMOVE_MANIFEST => {
//                 if config.manifests.is_empty() {
//                     colors::warning("No manifests in config to remove.");
//                     continue;
//                 }

//                 let names: Vec<String> = config.manifests.iter().map(|m| m.name.clone()).collect();
//                 let selection = inquire::Select::new("Select manifest to remove:", names)
//                     .prompt()?;

//                 if let Some(index) = config.manifests.iter().position(|m| m.name == selection) {
//                     config.manifests.remove(index);
//                     colors::success("Manifest removed from configuration.");
                    
//                     // save immediately
//                     confy::store("steam-uploader", None, &config)?;
//                     colors::success("Configuration saved");
//                 }
//             }

//             manifests_options::BACK => {
//                 return Ok(());
//             }

//             _ => {
//                 // Find which manifest was selected
//                 if let Some(pos) = options.iter().position(|o| o == &selection) {
//                     if pos < manifest_indices.len() {
//                         let index = manifest_indices[pos];
//                         let selected = &manifests[index];
//                         colors::info(&format!("Selected manifest: {} ({})", selected.name, selected.path));
//                     }
//                 }
//             }
//         }
//     }
// }


