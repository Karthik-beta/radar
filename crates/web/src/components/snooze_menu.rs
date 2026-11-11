use domain::api::SnoozeFor;
use leptos::prelude::*;

use super::ui::dropdown_menu::{
    DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuShortcut,
};

/// The two snooze lengths (F9). Keys `d` and `w` pick them through the key
/// reducer, and a click picks them here.
#[component]
pub fn SnoozeMenu(
    #[prop(into)] open: Signal<bool>,
    #[prop(into)] on_pick: Callback<SnoozeFor>,
) -> impl IntoView {
    view! {
        <DropdownMenu open=open class="snooze-menu">
            <DropdownMenuContent label="Snooze">
                <DropdownMenuItem on_select=move |_| on_pick.run(SnoozeFor::Day)>
                    "1 day"
                    <DropdownMenuShortcut>"d"</DropdownMenuShortcut>
                </DropdownMenuItem>
                <DropdownMenuItem on_select=move |_| on_pick.run(SnoozeFor::Week)>
                    "1 week"
                    <DropdownMenuShortcut>"w"</DropdownMenuShortcut>
                </DropdownMenuItem>
            </DropdownMenuContent>
        </DropdownMenu>
    }
}
