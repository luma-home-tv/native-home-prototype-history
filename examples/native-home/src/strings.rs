//! Typed application text for the initial English interface. Protocol keys and TV-returned
//! application names do not pass through this catalogue.
#[derive(Clone, Copy)]
pub enum Message {
    AppTitle, Applications, Controls, CurrentSettings, MoreSettings, OpenLgSettings,
    ManagedByLg, UnavailableContext, ViewOnly, PreviewFooter,
}
impl Message {
    pub const fn text(self) -> &'static str {
        match self {
            Self::AppTitle => "Native Home",
            Self::Applications => "Applications",
            Self::Controls => "Controls",
            Self::CurrentSettings => "Current settings",
            Self::MoreSettings => "More settings",
            Self::OpenLgSettings => "Open LG Settings",
            Self::ManagedByLg => "Managed by LG",
            Self::UnavailableContext => "Unavailable in the current input or picture mode",
            Self::ViewOnly => "View-only preview",
            Self::PreviewFooter => "Example values. Open LG Settings to make changes.",
        }
    }
}
