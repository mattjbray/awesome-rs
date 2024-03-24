use core_graphics::event::{CGEvent, CGEventFlags, CGEventType, EventField};

use crate::{mode::Mode, Layout};

#[derive(Debug)]
pub enum Action {
    ModeNormal,
    ModeInsert,
    RefreshWindowList,
    LayoutFloating,
    LayoutCascade,
    LayoutTiling,
    WindowFull,
    WindowLeftHalf,
    WindowRightHalf,
    NextWindow,
    PrevWindow,
    SwapNextWindow,
    SwapPrevWindow,
    IncrPrimaryColWidth,
    DecrPrimaryColWidth,
    IncrPrimaryColWindows,
    DecrPrimaryColWindows,
}

const KEYCODE_A: i64 = 0;
const KEYCODE_F: i64 = 3;
const KEYCODE_H: i64 = 4;
const KEYCODE_J: i64 = 38;
const KEYCODE_K: i64 = 40;
const KEYCODE_L: i64 = 37;
const KEYCODE_R: i64 = 15;
const KEYCODE_T: i64 = 17;
const KEYCODE_ENT: i64 = 36;
const FLG_CTL: CGEventFlags = CGEventFlags::CGEventFlagControl;
const FLG_ALT: CGEventFlags = CGEventFlags::CGEventFlagAlternate;

impl Action {
    pub fn of_cg_event(event: &CGEvent, mode: &Mode, layout: &Layout) -> Option<Self> {
        match event.get_type() {
            CGEventType::KeyDown => {
                let flags = event.get_flags();
                let keycode = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE);
                println!("KeyDown ({:?}) {}", mode, keycode);
                match (mode, keycode, layout) {
                    // CTRL + ALT + A
                    (Mode::Insert, KEYCODE_A, _) if flags.contains(FLG_CTL | FLG_ALT) => {
                        Some(Action::ModeNormal)
                    }
                    (Mode::Normal, KEYCODE_A, _) => Some(Action::LayoutCascade),
                    (Mode::Normal, KEYCODE_F, _) => Some(Action::LayoutFloating),
                    (Mode::Normal, KEYCODE_H, Layout::TileHorizontal(_))
                        if flags.contains(FLG_ALT) =>
                    {
                        Some(Action::DecrPrimaryColWindows)
                    }
                    (Mode::Normal, KEYCODE_L, Layout::TileHorizontal(_))
                        if flags.contains(FLG_ALT) =>
                    {
                        Some(Action::IncrPrimaryColWindows)
                    }
                    (Mode::Normal, KEYCODE_H, Layout::TileHorizontal(_)) => {
                        Some(Action::DecrPrimaryColWidth)
                    }
                    (Mode::Normal, KEYCODE_L, Layout::TileHorizontal(_)) => {
                        Some(Action::IncrPrimaryColWidth)
                    }
                    (Mode::Normal, KEYCODE_H, _) => Some(Action::WindowLeftHalf),
                    (Mode::Normal, KEYCODE_L, _) => Some(Action::WindowRightHalf),
                    (Mode::Normal, KEYCODE_R, _) => Some(Action::RefreshWindowList),
                    (Mode::Normal, KEYCODE_T, _) => Some(Action::LayoutTiling),
                    (Mode::Normal, KEYCODE_J, _) if flags.contains(FLG_ALT) => {
                        Some(Action::SwapNextWindow)
                    }
                    (Mode::Normal, KEYCODE_K, _) if flags.contains(FLG_ALT) => {
                        Some(Action::SwapPrevWindow)
                    }
                    (Mode::Normal, KEYCODE_J, _) => Some(Action::NextWindow),
                    (Mode::Normal, KEYCODE_K, _) => Some(Action::PrevWindow),
                    (Mode::Normal, KEYCODE_ENT, _) => Some(Action::WindowFull),
                    (Mode::Normal, _, _) => Some(Action::ModeInsert),
                    _ => None,
                }
            }
            _ => None,
        }
    }
}
