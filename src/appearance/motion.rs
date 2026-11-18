//! One application motion preference combined with the desktop's reduced-motion setting.
use std::fmt::Write as _;

#[cfg(test)]
mod tests;

pub const WORKSPACE_DURATION_MS: u64 = 160;
pub const WORKSPACE_EASING: &str = "cubic-bezier(0, 0, 0.2, 1)";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transition {
    pub duration_ms: u64,
    pub easing: &'static str,
}

impl Transition {
    pub const fn ms(duration_ms: u64, easing: &'static str) -> Self {
        Self {
            duration_ms,
            easing,
        }
    }

    pub fn css_value(self, reduced: bool) -> String {
        if reduced {
            "0ms".to_owned()
        } else {
            format!("{}ms {}", self.duration_ms, self.easing)
        }
    }
}

pub const WORKSPACE: Transition = Transition::ms(WORKSPACE_DURATION_MS, WORKSPACE_EASING);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Policy {
    pub enabled: bool,
    pub system_reduced: bool,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            enabled: true,
            system_reduced: false,
        }
    }
}

impl Policy {
    pub fn reduced(self) -> bool {
        !self.enabled || self.system_reduced
    }

    pub(crate) fn write_css_declarations(self, out: &mut String) {
        let reduced = self.reduced();
        let (fast, normal, slow) = if reduced {
            (0, 0, 0)
        } else {
            (100, WORKSPACE_DURATION_MS, 240)
        };
        let _ = writeln!(
            out,
            "    --gds-motion-reduced: {};",
            if reduced { 1 } else { 0 }
        );
        let _ = writeln!(out, "    --gds-motion-none: 0ms;");
        let _ = writeln!(out, "    --gds-motion-fast: {fast}ms;");
        let _ = writeln!(out, "    --gds-motion-normal: {normal}ms;");
        let _ = writeln!(out, "    --gds-motion-slow: {slow}ms;");
        if reduced {
            let _ = writeln!(out, "    --zui-motion-duration-fast: 0ms;");
            let _ = writeln!(out, "    --zui-motion-duration-normal: 0ms;");
            let _ = writeln!(out, "    --zui-motion-duration-slow: 0ms;");
            let _ = writeln!(out, "    --zui-motion-duration-slower: 0ms;");
            let _ = writeln!(out, "    --zui-motion-duration-slowest: 0ms;");
        }
        let _ = writeln!(
            out,
            "    --gds-motion-ease-standard: cubic-bezier(0.4, 0, 0.2, 1);"
        );
        let _ = writeln!(out, "    --gds-motion-ease-out: {WORKSPACE_EASING};");
        let _ = writeln!(out, "    --gds-motion-ease-in: cubic-bezier(0.4, 0, 1, 1);");
        let _ = writeln!(out, "    --gds-motion-ease-linear: linear;");
    }
}

pub fn enabled() -> bool {
    crate::app::Gds::current().appearance().motion().enabled()
}
