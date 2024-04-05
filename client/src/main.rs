/// Copyright (c) 2024 Algorealm

/// This crate is simply for quick-testing the swarmNL library APIs and assisting in developement. It is not the default or de-facto test crate/module, as it is only used
/// in-dev and will be removed subsequently.

pub static CONFIG_FILE_PATH: &str = "test_config.ini";

fn main() {}

#[cfg(test)]
mod tests {
    use ini::Ini;
    use std::{
        borrow::Cow,
        fmt::{Debug, Display},
    };

    use crate::CONFIG_FILE_PATH;

    /// try to read/write a byte vector to config file
    #[test]
    fn write_to_ini_file() {
        let test_vec = vec![
            12, 234, 45, 34, 54, 343, 43, 534, 43, 3423, 434, 43, 34, 667, 98,
        ];

        // try vec to `.ini` file
        assert!(write_config(
            "auth",
            "serialized_keypair",
            &format!("{:?}", test_vec)
        ));
        assert_eq!(
            read_config("auth", "serialized_keypair"),
            format!("{:?}", test_vec)
        );

        // test that we can read something after it
        assert_eq!(read_config("bio", "name"), "@thewoodfish");
    }

    /// read value from config file
    fn read_config(section: &str, key: &str) -> Cow<'static, str> {
        if let Ok(conf) = Ini::load_from_file(CONFIG_FILE_PATH) {
            if let Some(section) = conf.section(Some(section)) {
                if let Some(value) = section.get(key) {
                    return Cow::Owned(value.to_owned());
                }
            }
        }

        "".into()
    }

    /// write value into config file
    fn write_config(section: &str, key: &str, new_value: &str) -> bool {
        if let Ok(mut conf) = Ini::load_from_file(CONFIG_FILE_PATH) {
            // Set a value:
            conf.set_to(Some(section), key.into(), new_value.into());
            if let Ok(_) = conf.write_to_file(CONFIG_FILE_PATH) {
                return true;
            }
        }
        false
    }
}
