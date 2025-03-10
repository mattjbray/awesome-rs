use core_graphics::event::{CGEvent, CGEventFlags, CGEventType, EventField};

use crate::{mode::Mode, Layout};

#[derive(Debug)]
pub enum Action {
    ModeNormal,
    ModeInsert,
    ModeInsertNormal,
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
    MoveWindowToNextDisplay { follow: bool },
    MoveWindowToPrevDisplay { follow: bool },
    MoveWindowToGroup { id: u8, follow: bool },
    ToggleWindowInGroup(u8),
    ShowGroup(u8),
    NextGroup,
    PrevGroup,
    MoveWindowToNextGroup { follow: bool },
    MoveWindowToPrevGroup { follow: bool },
}

pub static HELP_TEXT: &str = "
+------+------------------------+---------------------------+
| mode | keys                   | action                    |
+------+-[modes]----------------+---------------------------+
| I    | <opt>+<shift> (hold)   | transient mode (T)        |
| T    | <opt>+<shift>+a        | normal mode (N)           |
| N    | <esc>/q                | insert mode (I)           |
+------+-[layouts]--------------+---------------------------+
| T/N  | t                      | tiling layout             |
| T/N  | f                      | floating layout           |
| T/N  | c                      | cascade layout            |
+------+-[motions]--------------+---------------------------+
| T/N  | j/k                    | window motion             |
| T/N  | i/o/0-9                | group motion              |
| T/N  | n/p                    | display motion            |
+------+-[window commands]------+---------------------------+
| N    | <opt>+[motion]         | move window               |
| N    | <opt>+<shift>+[motion] | move window and follow    |
| N    | <cmd>+[0-9]            | toggle window in group    |
| T/N  | <ret>                  | maximize window           |
| T/N  | m/M                    | minimize/restore window   |
| T/N  | h/l                    | window left/right half    |
+------+-[tiling commands]------+---------------------------+
| T/N  | h/l                    | adjust split width        |
| T/N  | <opt>+h/l              | number of primary windows |
+------+------------------------+---------------------------+
";

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
const KEYCODE_I: i64 = 34;
const KEYCODE_J: i64 = 38;
const KEYCODE_K: i64 = 40;
const KEYCODE_L: i64 = 37;
const KEYCODE_M: i64 = 46;
const KEYCODE_N: i64 = 45;
const KEYCODE_O: i64 = 31;
const KEYCODE_P: i64 = 35;
const KEYCODE_Q: i64 = 12;
const KEYCODE_R: i64 = 15;
const KEYCODE_T: i64 = 17;
const KEYCODE_X: i64 = 7;
const KEYCODE_ENT: i64 = 36;
const KEYCODE_ESC: i64 = 53;
const KEYCODE_F3: i64 = 160;
const FLG_NULL: CGEventFlags = CGEventFlags::CGEventFlagNull;
const FLG_CTRL: CGEventFlags = CGEventFlags::CGEventFlagControl;
const FLG_ALT: CGEventFlags = CGEventFlags::CGEventFlagAlternate;
const FLG_SHIFT: CGEventFlags = CGEventFlags::CGEventFlagShift;
const FLG_CMD: CGEventFlags = CGEventFlags::CGEventFlagCommand;

impl Action {
    pub fn of_cg_event(event: &CGEvent, mode: &Mode, layout: Option<&Layout>) -> Option<Self> {
        // Extract only relevant flags so we can use (==)
        let flags = event
            .get_flags()
            .intersection(FLG_CTRL | FLG_ALT | FLG_SHIFT | FLG_CMD);
        let nml_mode_flgs: CGEventFlags = FLG_ALT | FLG_SHIFT;
        match event.get_type() {
            CGEventType::FlagsChanged => {
                // eprintln!("FlagsChanged ({:?}) {:?}", mode, flags);
                match mode {
                    Mode::Insert if flags == nml_mode_flgs => Some(Self::ModeInsertNormal),
                    Mode::InsertNormal if flags != nml_mode_flgs => Some(Self::ModeInsert),
                    _ => None,
                }
            }
            CGEventType::KeyDown => {
                let keycode = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE);
                // eprintln!("KeyDown ({:?}) {}", mode, keycode);
                use Action::*;
                match (mode, flags, keycode, layout) {
                    (Mode::InsertNormal, _, KEYCODE_A, _) => Some(ModeNormal),
                    (Mode::Insert, _, KEYCODE_F3, _) => Some(ModeNormal),
                    (Mode::Normal, FLG_NULL, KEYCODE_C, _) => Some(LayoutCascade),
                    (Mode::InsertNormal, _, KEYCODE_C, _) => Some(LayoutCascade),
                    (Mode::Normal, FLG_NULL, KEYCODE_F, _) => Some(LayoutFloating),
                    (Mode::InsertNormal, _, KEYCODE_F, _) => Some(LayoutFloating),
                    (Mode::Normal, FLG_ALT, KEYCODE_H, Some(Layout::TileHorizontal(_))) => {
                        Some(IncrPrimaryColWindows)
                    }
                    (Mode::Normal, FLG_ALT, KEYCODE_L, Some(Layout::TileHorizontal(_))) => {
                        Some(DecrPrimaryColWindows)
                    }
                    (Mode::Normal, FLG_NULL, KEYCODE_H, Some(Layout::TileHorizontal(_))) => {
                        Some(DecrPrimaryColWidth)
                    }
                    (Mode::InsertNormal, _, KEYCODE_H, Some(Layout::TileHorizontal(_))) => {
                        Some(DecrPrimaryColWidth)
                    }
                    (Mode::Normal, FLG_NULL, KEYCODE_L, Some(Layout::TileHorizontal(_))) => {
                        Some(IncrPrimaryColWidth)
                    }
                    (Mode::InsertNormal, _, KEYCODE_L, Some(Layout::TileHorizontal(_))) => {
                        Some(IncrPrimaryColWidth)
                    }
                    (Mode::Normal, FLG_NULL, KEYCODE_H, _) => Some(WindowLeftHalf),
                    (Mode::InsertNormal, _, KEYCODE_H, _) => Some(WindowLeftHalf),
                    (Mode::Normal, FLG_NULL, KEYCODE_L, _) => Some(WindowRightHalf),
                    (Mode::InsertNormal, _, KEYCODE_L, _) => Some(WindowRightHalf),
                    (Mode::Normal, FLG_NULL, KEYCODE_M, _) => Some(WindowMinimize),
                    (Mode::InsertNormal, _, KEYCODE_M, _) => Some(WindowMinimize),
                    (Mode::Normal, FLG_SHIFT, KEYCODE_M, _) => Some(WindowRestore),
                    (Mode::Normal, FLG_NULL, KEYCODE_R, _) => Some(RelayoutAll),
                    (Mode::InsertNormal, _, KEYCODE_R, _) => Some(RelayoutAll),
                    (Mode::Normal, FLG_NULL, KEYCODE_T, _) => Some(LayoutTiling),
                    (Mode::InsertNormal, _, KEYCODE_T, _) => Some(LayoutTiling),
                    (Mode::Normal, FLG_ALT, KEYCODE_J, _) => Some(SwapNextWindow),
                    (Mode::Normal, FLG_ALT, KEYCODE_K, _) => Some(SwapPrevWindow),
                    (Mode::Normal, FLG_NULL, KEYCODE_J, _) => Some(NextWindow),
                    (Mode::InsertNormal, _, KEYCODE_J, _) => Some(NextWindow),
                    (Mode::Normal, FLG_NULL, KEYCODE_K, _) => Some(PrevWindow),
                    (Mode::InsertNormal, _, KEYCODE_K, _) => Some(PrevWindow),
                    (Mode::Normal, FLG_NULL, KEYCODE_ENT, _) => Some(WindowFull),
                    (Mode::InsertNormal, _, KEYCODE_ENT, _) => Some(WindowFull),
                    (Mode::Normal, FLG_NULL, KEYCODE_X, _) => Some(WindowClose),
                    (Mode::InsertNormal, _, KEYCODE_X, _) => Some(WindowClose),
                    (Mode::Normal, FLG_NULL, KEYCODE_N, _) => Some(NextDisplay),
                    (Mode::InsertNormal, _, KEYCODE_N, _) => Some(NextDisplay),
                    (Mode::Normal, FLG_NULL, KEYCODE_P, _) => Some(PrevDisplay),
                    (Mode::InsertNormal, _, KEYCODE_P, _) => Some(PrevDisplay),
                    (Mode::Normal, FLG_ALT, KEYCODE_N, _) => {
                        Some(MoveWindowToNextDisplay { follow: true })
                    }
                    (Mode::Normal, _, KEYCODE_N, _) if flags == FLG_ALT | FLG_SHIFT => {
                        Some(MoveWindowToNextDisplay { follow: false })
                    }
                    (Mode::Normal, FLG_ALT, KEYCODE_P, _) => {
                        Some(MoveWindowToPrevDisplay { follow: true })
                    }
                    (Mode::Normal, _, KEYCODE_P, _) if flags == FLG_ALT | FLG_SHIFT => {
                        Some(MoveWindowToPrevDisplay { follow: false })
                    }
                    (Mode::Normal, FLG_NULL, KEYCODE_I, _) => Some(PrevGroup),
                    (Mode::InsertNormal, _, KEYCODE_I, _) => Some(PrevGroup),
                    (Mode::Normal, FLG_NULL, KEYCODE_O, _) => Some(NextGroup),
                    (Mode::InsertNormal, _, KEYCODE_O, _) => Some(NextGroup),
                    (Mode::Normal, FLG_ALT, KEYCODE_I, _) => {
                        Some(MoveWindowToPrevGroup { follow: true })
                    }
                    (Mode::Normal, _, KEYCODE_I, _) if flags == FLG_ALT | FLG_SHIFT => {
                        Some(MoveWindowToPrevGroup { follow: false })
                    }
                    (Mode::Normal, FLG_ALT, KEYCODE_O, _) => {
                        Some(MoveWindowToNextGroup { follow: true })
                    }
                    (Mode::Normal, _, KEYCODE_O, _) if flags == FLG_ALT | FLG_SHIFT => {
                        Some(MoveWindowToNextGroup { follow: false })
                    }
                    (Mode::Normal, FLG_NULL, KEYCODE_0, _) => Some(ShowGroup(0)),
                    (Mode::InsertNormal, _, KEYCODE_0, _) => Some(ShowGroup(0)),
                    (Mode::Normal, FLG_ALT, KEYCODE_0, _) => Some(MoveWindowToGroup {
                        id: 0,
                        follow: true,
                    }),
                    (Mode::Normal, _, KEYCODE_0, _) if flags == FLG_ALT | FLG_SHIFT => {
                        Some(MoveWindowToGroup {
                            id: 0,
                            follow: false,
                        })
                    }
                    (Mode::Normal, FLG_CMD, KEYCODE_0, _) => Some(ToggleWindowInGroup(0)),
                    (Mode::Insert, _, KEYCODE_0, _) if flags == FLG_ALT | FLG_SHIFT => {
                        Some(ShowGroup(0))
                    }
                    (Mode::Normal, FLG_NULL, KEYCODE_1, _) => Some(ShowGroup(1)),
                    (Mode::InsertNormal, _, KEYCODE_1, _) => Some(ShowGroup(1)),
                    (Mode::Normal, FLG_ALT, KEYCODE_1, _) => Some(MoveWindowToGroup {
                        id: 1,
                        follow: true,
                    }),
                    (Mode::Normal, _, KEYCODE_1, _) if flags == FLG_ALT | FLG_SHIFT => {
                        Some(MoveWindowToGroup {
                            id: 1,
                            follow: false,
                        })
                    }
                    (Mode::Normal, FLG_CMD, KEYCODE_1, _) => Some(ToggleWindowInGroup(1)),
                    (Mode::Normal, FLG_NULL, KEYCODE_2, _) => Some(ShowGroup(2)),
                    (Mode::InsertNormal, _, KEYCODE_2, _) => Some(ShowGroup(2)),
                    (Mode::Normal, FLG_ALT, KEYCODE_2, _) => Some(MoveWindowToGroup {
                        id: 2,
                        follow: true,
                    }),
                    (Mode::Normal, _, KEYCODE_2, _) if flags == FLG_ALT | FLG_SHIFT => {
                        Some(MoveWindowToGroup {
                            id: 2,
                            follow: false,
                        })
                    }
                    (Mode::Normal, FLG_CMD, KEYCODE_2, _) => Some(ToggleWindowInGroup(2)),
                    (Mode::Normal, FLG_NULL, KEYCODE_3, _) => Some(ShowGroup(3)),
                    (Mode::InsertNormal, _, KEYCODE_3, _) => Some(ShowGroup(3)),
                    (Mode::Normal, FLG_ALT, KEYCODE_3, _) => Some(MoveWindowToGroup {
                        id: 3,
                        follow: true,
                    }),
                    (Mode::Normal, _, KEYCODE_3, _) if flags == FLG_ALT | FLG_SHIFT => {
                        Some(MoveWindowToGroup {
                            id: 3,
                            follow: false,
                        })
                    }
                    (Mode::Normal, FLG_CMD, KEYCODE_3, _) => Some(ToggleWindowInGroup(3)),
                    (Mode::Normal, FLG_NULL, KEYCODE_4, _) => Some(ShowGroup(4)),
                    (Mode::InsertNormal, _, KEYCODE_4, _) => Some(ShowGroup(4)),
                    (Mode::Normal, FLG_ALT, KEYCODE_4, _) => Some(MoveWindowToGroup {
                        id: 4,
                        follow: true,
                    }),
                    (Mode::Normal, _, KEYCODE_4, _) if flags == FLG_ALT | FLG_SHIFT => {
                        Some(MoveWindowToGroup {
                            id: 4,
                            follow: false,
                        })
                    }
                    (Mode::Normal, FLG_CMD, KEYCODE_4, _) => Some(ToggleWindowInGroup(4)),
                    (Mode::Normal, FLG_NULL, KEYCODE_5, _) => Some(ShowGroup(5)),
                    (Mode::InsertNormal, _, KEYCODE_5, _) => Some(ShowGroup(5)),
                    (Mode::Normal, FLG_ALT, KEYCODE_5, _) => Some(MoveWindowToGroup {
                        id: 5,
                        follow: true,
                    }),
                    (Mode::Normal, _, KEYCODE_5, _) if flags == FLG_ALT | FLG_SHIFT => {
                        Some(MoveWindowToGroup {
                            id: 5,
                            follow: false,
                        })
                    }
                    (Mode::Normal, FLG_CMD, KEYCODE_5, _) => Some(ToggleWindowInGroup(5)),
                    (Mode::Normal, FLG_NULL, KEYCODE_6, _) => Some(ShowGroup(6)),
                    (Mode::InsertNormal, _, KEYCODE_6, _) => Some(ShowGroup(6)),
                    (Mode::Normal, FLG_ALT, KEYCODE_6, _) => Some(MoveWindowToGroup {
                        id: 6,
                        follow: true,
                    }),
                    (Mode::Normal, _, KEYCODE_6, _) if flags == FLG_ALT | FLG_SHIFT => {
                        Some(MoveWindowToGroup {
                            id: 6,
                            follow: false,
                        })
                    }
                    (Mode::Normal, FLG_CMD, KEYCODE_6, _) => Some(ToggleWindowInGroup(6)),
                    (Mode::Normal, FLG_NULL, KEYCODE_7, _) => Some(ShowGroup(7)),
                    (Mode::InsertNormal, _, KEYCODE_7, _) => Some(ShowGroup(7)),
                    (Mode::Normal, FLG_ALT, KEYCODE_7, _) => Some(MoveWindowToGroup {
                        id: 7,
                        follow: true,
                    }),
                    (Mode::Normal, _, KEYCODE_7, _) if flags == FLG_ALT | FLG_SHIFT => {
                        Some(MoveWindowToGroup {
                            id: 7,
                            follow: false,
                        })
                    }
                    (Mode::Normal, FLG_CMD, KEYCODE_7, _) => Some(ToggleWindowInGroup(7)),
                    (Mode::Normal, FLG_NULL, KEYCODE_8, _) => Some(ShowGroup(8)),
                    (Mode::InsertNormal, _, KEYCODE_8, _) => Some(ShowGroup(8)),
                    (Mode::Normal, FLG_ALT, KEYCODE_8, _) => Some(MoveWindowToGroup {
                        id: 8,
                        follow: true,
                    }),
                    (Mode::Normal, _, KEYCODE_8, _) if flags == FLG_ALT | FLG_SHIFT => {
                        Some(MoveWindowToGroup {
                            id: 8,
                            follow: false,
                        })
                    }
                    (Mode::Normal, FLG_CMD, KEYCODE_8, _) => Some(ToggleWindowInGroup(8)),
                    (Mode::Normal, FLG_NULL, KEYCODE_9, _) => Some(ShowGroup(9)),
                    (Mode::InsertNormal, _, KEYCODE_9, _) => Some(ShowGroup(9)),
                    (Mode::Normal, FLG_ALT, KEYCODE_9, _) => Some(MoveWindowToGroup {
                        id: 9,
                        follow: true,
                    }),
                    (Mode::Normal, _, KEYCODE_9, _) if flags == FLG_ALT | FLG_SHIFT => {
                        Some(MoveWindowToGroup {
                            id: 9,
                            follow: false,
                        })
                    }
                    (Mode::Normal, FLG_CMD, KEYCODE_9, _) => Some(ToggleWindowInGroup(9)),
                    (Mode::Normal, _, KEYCODE_ESC | KEYCODE_Q | KEYCODE_F3, _) => Some(ModeInsert),
                    _ => None,
                }
            }
            _ => None,
        }
    }
}
