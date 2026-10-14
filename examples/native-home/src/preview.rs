//! Native visual development surface for the control centre.
//! This static capture utility has no input dispatcher or TV action executor. The eventual
//! interactive application will mount these shared forms through the shared screen/focus system.

use std::ffi::CString;

use plx_ui::{Painter, Rect, theme};
use plx_ui::form::{Form, FormSection, FormTable, RowKey, RowKind};
use plx_ui::label::{Label, HAlign};
use plx_ui::table::Row;
use plx_ui::widgets::LegacyMeasure;

use crate::model::{Action, Context, Page, Setting, controls_form};
use crate::strings::Message;

const HEADER: Rect = Rect::new(96.0, 64.0, 1320.0, 76.0);
const CONTEXT: Rect = Rect::new(96.0, 150.0, 1450.0, 46.0);
const CATEGORY_FRAME: Rect = Rect::new(96.0, 250.0, 430.0, 690.0);
const DETAIL_FRAME: Rect = Rect::new(646.0, 250.0, 1178.0, 690.0);
const FOOTER: Rect = Rect::new(96.0, 977.0, 1728.0, 44.0);

pub struct ControlPreview {
    categories: FormTable<String, Action, Page>,
    controls: FormTable<String, Action, Page>,
    title: CString,
    context: CString,
    footer: CString,
    capability: CString,
}

impl ControlPreview {
    pub fn new() -> Self {
        let mut categories = FormSection::new(Message::Controls.text());
        for (index, (title, page)) in [
            ("Picture", Page::Picture), ("Sound", Page::Sound), ("Network", Page::Network),
            ("Accessibility", Page::Accessibility), ("General", Page::General), ("Panel care", Page::PanelCare),
        ].into_iter().enumerate() {
            categories = categories.item_keyed(title.to_owned(), RowKey(index as u32 + 1),
                RowKind::Nav(page), Action::Inspect(title.to_owned()), Row::new(title));
        }
        let mut category_table = FormTable::new(2000);
        category_table.set(Form::new().section(categories), None);
        // The detail column is visible without owning focus, avoiding two competing highlights.
        let ctx = Context { input: "HDMI_1".into(), dynamic_range: "sdr".into(), picture_mode: "cinema".into() };
        let items = [
            ("pictureMode", "Picture mode", "Cinema"),
            ("backlight", "OLED pixel brightness", "75"),
            ("contrast", "Contrast", "85"),
            ("brightness", "Black level", "50"),
        ].into_iter().enumerate().map(|(index, (id, label, value))| Setting {
            id: id.to_owned(), focus_key: index as u32 + 1, label: label.to_owned(), value: value.to_owned(),
            context: ctx.clone(), protected: false, available: true,
        }).collect::<Vec<_>>();
        let mut controls = FormTable::new(2000);
        controls.set(controls_form(&items, &ctx), None);
        controls.table.list_focused = false;
        Self {
            categories: category_table, controls,
            title: CString::new(Message::AppTitle.text()).unwrap(),
            context: CString::new("Picture  /  HDMI 1  /  SDR  /  Cinema").unwrap(),
            footer: CString::new(Message::PreviewFooter.text()).unwrap(),
            capability: CString::new(Message::ViewOnly.text()).unwrap(),
        }
    }

    pub fn draw(&mut self, dt: f32) {
        let p = Painter::root();
        let ground = theme::SURFACE_APP;
        plx_gfx::gfx::frame_clear(ground[0], ground[1], ground[2]);
        Label::new(self.title.as_ptr(), theme::size::DISPLAY, theme::TEXT_PRIMARY).bold().draw(p, HEADER);
        Label::new(self.capability.as_ptr(), theme::size::CAPTION, theme::TEXT_SECONDARY)
            .h(HAlign::Right).draw(p, Rect::new(1470.0, 64.0, 354.0, 76.0));
        Label::new(self.context.as_ptr(), theme::size::BODY, theme::TEXT_SECONDARY).draw(p, CONTEXT);
        p.rect(Rect::new(96.0, 213.0, 1728.0, 1.0), 0.0, theme::HAIRLINE, theme::HAIRLINE, 0.0);
        self.categories.table.update(dt, CATEGORY_FRAME.h);
        self.controls.table.update(dt, DETAIL_FRAME.h);
        self.categories.table.draw_page(p, CATEGORY_FRAME, &LegacyMeasure);
        self.controls.table.draw_page(p, DETAIL_FRAME, &LegacyMeasure);
        Label::new(self.footer.as_ptr(), theme::size::CAPTION, theme::TEXT_TERTIARY)
            .h(HAlign::Left).draw(p, FOOTER);
    }
}

impl Default for ControlPreview {
    fn default() -> Self { Self::new() }
}
