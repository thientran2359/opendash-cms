mod extension_settings;
mod general;
mod locales;
mod public_site;
mod reading;
mod theme_options;

pub use extension_settings::*;
pub use general::*;
pub use locales::*;
pub use public_site::*;
pub use reading::*;
pub use theme_options::*;

#[cfg(test)]
#[path = "tests/general_and_options_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/extension_settings_tests.rs"]
mod plugin_settings_s02_tests;
