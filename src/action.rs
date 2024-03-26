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
    WindowMinimize,
    WindowRestore,
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
const KEYCODE_C: i64 = 8;
const KEYCODE_F: i64 = 3;
const KEYCODE_H: i64 = 4;
const KEYCODE_J: i64 = 38;
const KEYCODE_K: i64 = 40;
const KEYCODE_L: i64 = 37;
const KEYCODE_M: i64 = 46;
const KEYCODE_R: i64 = 15;
const KEYCODE_T: i64 = 17;
const KEYCODE_ENT: i64 = 36;
const FLG_NULL: CGEventFlags = CGEventFlags::CGEventFlagNull;
const FLG_CTRL: CGEventFlags = CGEventFlags::CGEventFlagControl;
const FLG_ALT: CGEventFlags = CGEventFlags::CGEventFlagAlternate;
const FLG_SHIFT: CGEventFlags = CGEventFlags::CGEventFlagShift;
const FLG_CMD: CGEventFlags = CGEventFlags::CGEventFlagCommand;

impl Action {
    pub fn of_cg_event(event: &CGEvent, mode: &Mode, layout: &Layout) -> Option<Self> {
        match event.get_type() {
            CGEventType::KeyDown => {
                // Extract only relevant flags so we can use (==)
                let flags = event
                    .get_flags()
                    .intersection(FLG_CTRL | FLG_ALT | FLG_SHIFT | FLG_CMD);
                let keycode = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE);
                println!("KeyDown ({:?}) {}", mode, keycode);
                use Action::*;
                match (mode, flags, keycode, layout) {
                    (Mode::Insert, _, KEYCODE_A, _) if flags == FLG_SHIFT | FLG_CMD => {
                        Some(ModeNormal)
                    }
                    (Mode::Normal, FLG_NULL, KEYCODE_C, _) => Some(LayoutCascade),
                    (Mode::Normal, FLG_NULL, KEYCODE_F, _) => Some(LayoutFloating),
                    (Mode::Normal, FLG_ALT, KEYCODE_H, Layout::TileHorizontal(_)) => {
                        Some(IncrPrimaryColWindows)
                    }
                    (Mode::Normal, FLG_ALT, KEYCODE_L, Layout::TileHorizontal(_)) => {
                        Some(DecrPrimaryColWindows)
                    }
                    (Mode::Normal, FLG_NULL, KEYCODE_H, Layout::TileHorizontal(_)) => {
                        Some(DecrPrimaryColWidth)
                    }
                    (Mode::Normal, FLG_NULL, KEYCODE_L, Layout::TileHorizontal(_)) => {
                        Some(IncrPrimaryColWidth)
                    }
                    (Mode::Normal, FLG_NULL, KEYCODE_H, _) => Some(WindowLeftHalf),
                    (Mode::Normal, FLG_NULL, KEYCODE_L, _) => Some(WindowRightHalf),
                    (Mode::Normal, FLG_NULL, KEYCODE_M, _) => Some(WindowMinimize),
                    (Mode::Normal, FLG_SHIFT, KEYCODE_M, _) => Some(WindowRestore),
                    (Mode::Normal, FLG_NULL, KEYCODE_R, _) => Some(RefreshWindowList),
                    (Mode::Normal, FLG_NULL, KEYCODE_T, _) => Some(LayoutTiling),
                    (Mode::Normal, FLG_ALT, KEYCODE_J, _) => Some(SwapNextWindow),
                    (Mode::Normal, FLG_ALT, KEYCODE_K, _) => Some(SwapPrevWindow),
                    (Mode::Normal, FLG_NULL, KEYCODE_J, _) => Some(NextWindow),
                    (Mode::Normal, FLG_NULL, KEYCODE_K, _) => Some(PrevWindow),
                    (Mode::Normal, FLG_NULL, KEYCODE_ENT, _) => Some(WindowFull),
                    (Mode::Normal, _, _, _) => Some(ModeInsert),
                    _ => None,
                }
            }
            _ => None,
        }
    }
}
