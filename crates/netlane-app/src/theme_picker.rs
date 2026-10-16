//! Seletor de temas como o do Zed: filtro, lista, prévia ao navegar pelo teclado.
//! Enter confirma; Esc ou clique fora volta ao tema anterior.

use gpui::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, KeyBinding, SharedString,
    Subscription, Window, actions, div, prelude::*, px,
};

use crate::{
    scrollbar::ScrollbarExt,
    text_input::{TextInput, TextInputEvent},
    theme::{ActiveTheme, Themes},
    ui::icon,
};

actions!(theme_picker, [SelectNext, SelectPrevious]);

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("down", SelectNext, Some("ThemePicker")),
        KeyBinding::new("up", SelectPrevious, Some("ThemePicker")),
    ]);
}

pub enum ThemePickerEvent {
    Previewed,
    Closed,
}

pub struct ThemePicker {
    error: Option<String>,
    filter: Entity<TextInput>,
    names: Vec<SharedString>,
    matches: Vec<usize>,
    highlighted: usize,
    original: SharedString,
    _subscription: Subscription,
}

impl EventEmitter<ThemePickerEvent> for ThemePicker {}

impl ThemePicker {
    #[cfg(feature = "gui-test")]
    pub fn proof_filter(&mut self, text: &str, cx: &mut Context<Self>) {
        self.filter.update(cx, |input, cx| input.set_text(text, cx));
        self.refilter(cx);
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_cancel(&mut self, cx: &mut Context<Self>) {
        self.cancel(cx);
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_confirm(&mut self, cx: &mut Context<Self>) {
        self.confirm(cx);
    }
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let names: Vec<SharedString> = cx
            .global::<Themes>()
            .all
            .iter()
            .map(|t| t.name.clone())
            .collect();
        let original = cx.theme().name.clone();
        let filter = cx.new(|cx| TextInput::new("", "Selecionar tema…", cx));
        let subscription =
            cx.subscribe_in(&filter, window, |picker, _, event, _, cx| match event {
                TextInputEvent::Changed => picker.refilter(cx),
                TextInputEvent::Confirmed => picker.confirm(cx),
                TextInputEvent::Cancelled => picker.cancel(cx),
            });
        let highlighted = names.iter().position(|n| *n == original).unwrap_or(0);
        Self {
            error: None,
            filter,
            matches: (0..names.len()).collect(),
            names,
            highlighted,
            original,
            _subscription: subscription,
        }
    }

    fn refilter(&mut self, cx: &mut Context<Self>) {
        let query = self.filter.read(cx).text().to_lowercase();
        self.matches = (0..self.names.len())
            .filter(|&i| self.names[i].to_lowercase().contains(&query))
            .collect();
        self.highlighted = 0;
        self.preview(cx);
    }

    fn preview(&mut self, cx: &mut Context<Self>) {
        if let Some(&i) = self.matches.get(self.highlighted) {
            Themes::set_active(cx, &self.names[i]);
            cx.emit(ThemePickerEvent::Previewed);
        }
        cx.notify();
    }

    fn step(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.matches.is_empty() {
            return;
        }
        let len = self.matches.len() as isize;
        self.highlighted = (self.highlighted as isize + delta).rem_euclid(len) as usize;
        self.preview(cx);
    }

    fn confirm(&mut self, cx: &mut Context<Self>) {
        if self.matches.get(self.highlighted).is_none() {
            Themes::set_active(cx, &self.original);
        }
        // O tema escolhido vale nas próximas aberturas do app.
        let name = cx.theme().name.to_string();
        if let Err(error) = crate::settings::AppSettings::update(cx, |s| s.theme = Some(name)) {
            self.error = Some(format!("Não foi possível salvar o tema: {error:#}"));
            cx.notify();
            return;
        }
        cx.emit(ThemePickerEvent::Closed);
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        Themes::set_active(cx, &self.original);
        cx.emit(ThemePickerEvent::Closed);
    }
}

impl Focusable for ThemePicker {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.filter.focus_handle(cx)
    }
}

impl Render for ThemePicker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let hover = t.hover;
        let mut list = div()
            .id("theme-list")
            .max_h(px(420.))
            .overflow_y_scroll()
            .py(px(4.))
            .flex()
            .flex_col();
        for (pos, &i) in self.matches.iter().enumerate() {
            let name = self.names[i].clone();
            let highlighted = pos == self.highlighted;
            let is_original = name == self.original;
            list = list.child(
                div()
                    .id(("theme", i))
                    .h(px(30.))
                    .mx(px(4.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .rounded(px(4.))
                    .cursor_pointer()
                    .text_color(if highlighted { t.text_strong } else { t.text })
                    .when(highlighted, |d| d.bg(t.selected))
                    .when(!highlighted, |d| d.hover(move |s| s.bg(hover)))
                    .on_click(cx.listener(move |picker, _, _, cx| {
                        picker.highlighted = pos;
                        picker.preview(cx);
                        picker.original = picker.names[i].clone();
                        picker.confirm(cx);
                    }))
                    .child(div().flex_1().child(name))
                    .when(is_original, |d| d.child(icon("check", 16., t.text_muted))),
            );
        }
        if self.matches.is_empty() {
            list = list.child(
                div()
                    .px(px(12.))
                    .py(px(8.))
                    .text_color(t.text_muted)
                    .child("Nenhum tema encontrado"),
            );
        }

        div()
            .key_context("ThemePicker")
            .occlude()
            .w(px(540.))
            .flex()
            .flex_col()
            .rounded(px(6.))
            .bg(t.elevated)
            .border_1()
            .border_color(t.border)
            .shadow_lg()
            .when_some(self.error.clone(), |d, error| {
                d.child(div().p(px(12.)).text_color(t.error).child(error))
            })
            .on_action(cx.listener(|picker, _: &SelectNext, _, cx| picker.step(1, cx)))
            .on_action(cx.listener(|picker, _: &SelectPrevious, _, cx| picker.step(-1, cx)))
            .on_mouse_down_out(cx.listener(|picker, _, _, cx| picker.cancel(cx)))
            .child(
                div()
                    .h(px(36.))
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .border_b_1()
                    .border_color(t.divider)
                    .child(self.filter.clone()),
            )
            .child(list.with_scrollbar(cx))
            .child(
                div()
                    .h(px(34.))
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_t_1()
                    .border_color(t.divider)
                    .text_size(px(11.5))
                    .text_color(t.text_muted)
                    .child("↑ ↓ prévia · Enter confirma · Esc cancela"),
            )
    }
}
