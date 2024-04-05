use core_graphics::event::{CGEvent, CGEventFlags, CGEventType, EventField};

use crate::{mode::Mode, Layout};

#[derive(Debug)]
pub enum Action {
    ModeNormal,
    ModeInsert,
    RelayoutAll,
    LayoutFloating,
    LayoutCascade,
    LayoutTiling,
    WindowFull,
    WindowLeftHalf,
    WindowRightHalf,
    WindowMinimize,
    WindowRestore,
    WindowClose,
    NextWindow,
    PrevWindow,
    SwapNextWindow,
    SwapPrevWindow,
    IncrPrimaryColWidth,
    DecrPrimaryColWidth,
    IncrPrimaryColWindows,
    DecrPrimaryColWindows,
    NextDisplay,
    PrevDisplay,
    MoveWindowToNextDisplay,
    MoveWindowToPrevDisplay,
    MoveWindowToGroup(u8),
    ToggleWindowInGroup(u8),
    ShowGroup(u8),
}

const KEYCODE_0: i64 = 29;
const KEYCODE_1: i64 = 18;
const KEYCODE_2: i64 = 19;
const KEYCODE_3: i64 = 20;
const KEYCODE_4: i64 = 21;
const KEYCODE_5: i64 = 23;
const KEYCODE_6: i64 = 22;
const KEYCODE_7: i64 = 26;
const KEYCODE_8: i64 = 28;
const KEYCODE_9: i64 = 25;
const KEYCODE_A: i64 = 0;
const KEYCODE_C: i64 = 8;
const KEYCODE_F: i64 = 3;
const KEYCODE_H: i64 = 4;
const KEYCODE_J: i64 = 38;
const KEYCODE_K: i64 = 40;
const KEYCODE_L: i64 = 37;
const KEYCODE_M: i64 = 46;
const KEYCODE_N: i64 = 45;
const KEYCODE_P: i64 = 35;
const KEYCODE_R: i64 = 15;
const KEYCODE_T: i64 = 17;
const KEYCODE_X: i64 = 7;
const KEYCODE_ENT: i64 = 36;
const FLG_NULL: CGEventFlags = CGEventFlags::CGEventFlagNull;
const FLG_CTRL: CGEventFlags = CGEventFlags::CGEventFlagControl;
const FLG_ALT: CGEventFlags = CGEventFlags::CGEventFlagAlternate;
const FLG_SHIFT: CGEventFlags = CGEventFlags::CGEventFlagShift;
const FLG_CMD: CGEventFlags = CGEventFlags::CGEventFlagCommand;

impl Action {
    pub fn of_cg_event(event: &CGEvent, mode: &Mode, layout: Option<&Layout>) -> Option<Self> {
        match event.get_type() {
            CGEventType::KeyDown => {
                // Extract only relevant flags so we can use (==)
                let flags = event
                    .get_flags()
                    .intersection(FLG_CTRL | FLG_ALT | FLG_SHIFT | FLG_CMD);
                let keycode = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE);
                // eprintln!("KeyDown ({:?}) {}", mode, keycode);
                use Action::*;
                match (mode, flags, keycode, layout) {
                    (Mode::Insert, _, KEYCODE_A, _) if flags == FLG_SHIFT | FLG_ALT => {
                        Some(ModeNormal)
                    }
                    (Mode::Normal, FLG_NULL, KEYCODE_C, _) => Some(LayoutCascade),
                    (Mode::Normal, FLG_NULL, KEYCODE_F, _) => Some(LayoutFloating),
                    (Mode::Normal, FLG_ALT, KEYCODE_H, Some(Layout::TileHorizontal(_))) => {
                        Some(IncrPrimaryColWindows)
                    }
                    (Mode::Normal, FLG_ALT, KEYCODE_L, Some(Layout::TileHorizontal(_))) => {
                        Some(DecrPrimaryColWindows)
                    }
                    (Mode::Normal, FLG_NULL, KEYCODE_H, Some(Layout::TileHorizontal(_))) => {
                        Some(DecrPrimaryColWidth)
                    }
                    (Mode::Normal, FLG_NULL, KEYCODE_L, Some(Layout::TileHorizontal(_))) => {
                        Some(IncrPrimaryColWidth)
                    }
                    (Mode::Normal, FLG_NULL, KEYCODE_H, _) => Some(WindowLeftHalf),
                    (Mode::Normal, FLG_NULL, KEYCODE_L, _) => Some(WindowRightHalf),
                    (Mode::Normal, FLG_NULL, KEYCODE_M, _) => Some(WindowMinimize),
                    (Mode::Normal, FLG_SHIFT, KEYCODE_M, _) => Some(WindowRestore),
                    (Mode::Normal, FLG_NULL, KEYCODE_R, _) => Some(RelayoutAll),
                    (Mode::Normal, FLG_NULL, KEYCODE_T, _) => Some(LayoutTiling),
                    (Mode::Normal, FLG_ALT, KEYCODE_J, _) => Some(SwapNextWindow),
                    (Mode::Normal, FLG_ALT, KEYCODE_K, _) => Some(SwapPrevWindow),
                    (Mode::Normal, FLG_NULL, KEYCODE_J, _) => Some(NextWindow),
                    (Mode::Normal, FLG_NULL, KEYCODE_K, _) => Some(PrevWindow),
                    (Mode::Normal, FLG_NULL, KEYCODE_ENT, _) => Some(WindowFull),
                    (Mode::Normal, FLG_NULL, KEYCODE_X, _) => Some(WindowClose),
                    (Mode::Normal, FLG_NULL, KEYCODE_N, _) => Some(NextDisplay),
                    (Mode::Normal, FLG_NULL, KEYCODE_P, _) => Some(PrevDisplay),
                    (Mode::Normal, FLG_ALT, KEYCODE_N, _) => Some(MoveWindowToNextDisplay),
                    (Mode::Normal, FLG_ALT, KEYCODE_P, _) => Some(MoveWindowToPrevDisplay),
                    (Mode::Normal, FLG_NULL, KEYCODE_0, _) => Some(ShowGroup(0)),
                    (Mode::Normal, FLG_ALT, KEYCODE_0, _) => Some(MoveWindowToGroup(0)),
                    (Mode::Normal, _, KEYCODE_0, _) if flags == FLG_ALT | FLG_SHIFT => {
                        Some(ToggleWindowInGroup(0))
                    }
                    (Mode::Normal, FLG_NULL, KEYCODE_1, _) => Some(ShowGroup(1)),
                    (Mode::Normal, FLG_ALT, KEYCODE_1, _) => Some(MoveWindowToGroup(1)),
                    (Mode::Normal, _, KEYCODE_1, _) if flags == FLG_ALT | FLG_SHIFT => {
                        Some(ToggleWindowInGroup(1))
                    }
                    (Mode::Normal, FLG_NULL, KEYCODE_2, _) => Some(ShowGroup(2)),
                    (Mode::Normal, FLG_ALT, KEYCODE_2, _) => Some(MoveWindowToGroup(2)),
                    (Mode::Normal, _, KEYCODE_2, _) if flags == FLG_ALT | FLG_SHIFT => {
                        Some(ToggleWindowInGroup(2))
                    }
                    (Mode::Normal, FLG_NULL, KEYCODE_3, _) => Some(ShowGroup(3)),
                    (Mode::Normal, FLG_ALT, KEYCODE_3, _) => Some(MoveWindowToGroup(3)),
                    (Mode::Normal, _, KEYCODE_3, _) if flags == FLG_ALT | FLG_SHIFT => {
                        Some(ToggleWindowInGroup(3))
                    }
                    (Mode::Normal, FLG_NULL, KEYCODE_4, _) => Some(ShowGroup(4)),
                    (Mode::Normal, FLG_ALT, KEYCODE_4, _) => Some(MoveWindowToGroup(4)),
                    (Mode::Normal, _, KEYCODE_4, _) if flags == FLG_ALT | FLG_SHIFT => {
                        Some(ToggleWindowInGroup(4))
                    }
                    (Mode::Normal, FLG_NULL, KEYCODE_5, _) => Some(ShowGroup(5)),
                    (Mode::Normal, FLG_ALT, KEYCODE_5, _) => Some(MoveWindowToGroup(5)),
                    (Mode::Normal, _, KEYCODE_5, _) if flags == FLG_ALT | FLG_SHIFT => {
                        Some(ToggleWindowInGroup(5))
                    }
                    (Mode::Normal, FLG_NULL, KEYCODE_6, _) => Some(ShowGroup(6)),
                    (Mode::Normal, FLG_ALT, KEYCODE_6, _) => Some(MoveWindowToGroup(6)),
                    (Mode::Normal, _, KEYCODE_6, _) if flags == FLG_ALT | FLG_SHIFT => {
                        Some(ToggleWindowInGroup(6))
                    }
                    (Mode::Normal, FLG_NULL, KEYCODE_7, _) => Some(ShowGroup(7)),
                    (Mode::Normal, FLG_ALT, KEYCODE_7, _) => Some(MoveWindowToGroup(7)),
                    (Mode::Normal, _, KEYCODE_7, _) if flags == FLG_ALT | FLG_SHIFT => {
                        Some(ToggleWindowInGroup(7))
                    }
                    (Mode::Normal, FLG_NULL, KEYCODE_8, _) => Some(ShowGroup(8)),
                    (Mode::Normal, FLG_ALT, KEYCODE_8, _) => Some(MoveWindowToGroup(8)),
                    (Mode::Normal, _, KEYCODE_8, _) if flags == FLG_ALT | FLG_SHIFT => {
                        Some(ToggleWindowInGroup(8))
                    }
                    (Mode::Normal, FLG_NULL, KEYCODE_9, _) => Some(ShowGroup(9)),
                    (Mode::Normal, FLG_ALT, KEYCODE_9, _) => Some(MoveWindowToGroup(9)),
                    (Mode::Normal, _, KEYCODE_9, _) if flags == FLG_ALT | FLG_SHIFT => {
                        Some(ToggleWindowInGroup(9))
                    }
                    (Mode::Normal, _, _, _) => Some(ModeInsert),
                    _ => None,
                }
            }
            _ => None,
        }
    }
}
