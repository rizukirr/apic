//! What a picked folder means, as a pure decision. No filesystem, process,
//! thread or egui.

/// Why a folder picked with Open cannot be opened, or `None` when it can.
/// Open only accepts an existing project (one with `.apic/config.toml`):
/// New project is what creates one, so Open never initializes a folder.
///
/// # Arguments
///
/// * `has_config` - whether the folder holds `.apic/config.toml`
/// * `location` - the folder as shown to the user
pub(crate) fn open_refusal(has_config: bool, location: &str) -> Option<String> {
    (!has_config)
        .then(|| format!("Not an apic project: {location}. Use New project to create one."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_with_a_config_opens() {
        assert_eq!(open_refusal(true, "~/p"), None);
    }

    #[test]
    fn a_folder_without_a_config_is_refused_with_the_new_project_hint() {
        let refusal = open_refusal(false, "~/p").expect("refused");
        assert!(refusal.contains("~/p"));
        assert!(refusal.contains("New project"));
    }
}
