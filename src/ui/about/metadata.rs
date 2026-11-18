//! Application identity embedded from Cargo metadata at build time.

#[derive(Debug)]
pub struct Application {
    pub binary: &'static str,
    pub name: &'static str,
    pub icon: &'static str,
    pub version: &'static str,
    pub build_tag: &'static str,
    pub website: &'static str,
    pub feedback: &'static str,
    pub authors: &'static [&'static str],
    pub credits: &'static [Credit],
    pub license: &'static str,
    pub license_text: &'static str,
}

#[derive(Debug)]
pub struct Credit {
    pub name: &'static str,
    pub website: &'static str,
}

include!(concat!(env!("OUT_DIR"), "/about.rs"));

impl Application {
    /// The entry for an application registered in Cargo metadata.
    ///
    /// # Panics
    /// Panics if `binary` is not a registered target of this package.
    pub fn cargo(binary: &str) -> &'static Self {
        Self::try_cargo(binary).expect("Cargo binary has registered application metadata")
    }

    /// Look up an application registered in Cargo metadata by binary target name.
    #[must_use]
    pub fn try_cargo(binary: &str) -> Option<&'static Self> {
        APPLICATIONS
            .iter()
            .find(|application| application.binary == binary)
    }

    /// Embedded monochrome solid SVG brand artwork corresponding to [`Application::icon`].
    #[must_use]
    pub const fn artwork_svg(&self) -> Option<&'static str> {
        match self.icon.as_bytes() {
            b"assets/files/icon-solid.svg" => {
                Some(include_str!("../../../assets/files/icon-solid.svg"))
            }
            b"assets/agents/icon-solid.svg" => {
                Some(include_str!("../../../assets/agents/icon-solid.svg"))
            }
            b"assets/images/icon-solid.svg" => {
                Some(include_str!("../../../assets/images/icon-solid.svg"))
            }
            _ => None,
        }
    }
}
