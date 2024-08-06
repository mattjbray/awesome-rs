#![deny(unsafe_op_in_unsafe_fn)]

use std::{
    cell::{Cell, RefCell},
    marker::PhantomData,
};

use objc2::{
    declare_class, msg_send, msg_send_id, mutability,
    rc::Retained,
    runtime::{NSObject, ProtocolObject, Sel},
    ClassType, DeclaredClass, Message, RefEncode,
};
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate};
use objc2_foundation::{MainThreadMarker, NSNotification, NSObjectProtocol};

use crate::window_manager::WindowManager;

// #[derive(Debug)]
// #[allow(unused)]
struct Ivars<'a> {
    state: RefCell<WindowManager>,
    event_tap: Cell<Option<core_graphics::event::CGEventTap<'a>>>,
    loop_source: Cell<Option<core_foundation::runloop::CFRunLoopSource>>,
}

#[repr(C)]
struct AppDelegate<'a> {
    superclass: NSObject,
    p: PhantomData<&'a mut u8>,
}

unsafe impl RefEncode for AppDelegate<'a> {
    const ENCODING_REF: objc2::Encoding = NSObject::ENCODING_REF;
}

unsafe impl Message for AppDelegate<'_> {}

unsafe impl ClassType for AppDelegate {
    type Super = NSObject;
    type Mutability = mutability::MainThreadOnly;
    const NAME: &'static str = "MyAppDelegate";
}

impl DeclaredClass for AppDelegate {
    type Ivars = Ivars<'a>;
}

unsafe impl NSObjectProtocol for AppDelegate {}
unsafe impl NSApplicationDelegate for AppDelegate {
    #[method(applicationDidFinishLaunching:)]
    fn did_finish_launching(&self, notification: &NSNotification) {
        println!("Did finish launching!");
        dbg!(notification);
        let (event_tap, loop_source) = crate::event_loop::create_event_loop(&self.ivars().state);
        let ivars = self.ivars();
        self.set_ivars(Ivars {
            state,
            event_tap: Some(event_tap),
            loop_source,
        })
    }

    #[method(applicationWillTerminate:)]
    fn will_terminate(&self, _notification: &NSNotification) {
        println!("Will terminate!");
    }
}

impl<'a> AppDelegate<'a> {
    unsafe extern "C" fn init_with_ptr<'s>(
        &'s mut self,
        _cmd: Sel,
        ptr: Option<&'a mut u8>,
    ) -> Option<&'s mut Self> {
        let this: Option<&mut Self> = unsafe { msg_send![super(self), init] };
        this.map(|this| {
            let ivar = Self::class().instance_variable("number").unwrap();
            // SAFETY: The ivar is added with the same type below
            unsafe {
                ivar.load_ptr::<&mut u8>(&this.superclass)
                    .write(ptr.expect("got NULL number ptr"))
            };
            this
        })
    }
    fn new(state: RefCell<WindowManager>, mtm: MainThreadMarker) -> Retained<Self> {
        let this = mtm.alloc();
        let this = this.set_ivars(Ivars { state });
        unsafe { msg_send_id![super(this), init] }
    }
}

pub fn main() {
    let mtm = MainThreadMarker::new().unwrap();
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);

    let wm = WindowManager::new();
    let state: RefCell<WindowManager> = RefCell::new(wm);

    let delegate = AppDelegate::new(state, mtm);
    let object = ProtocolObject::from_ref(&*delegate);
    app.setDelegate(Some(object));

    unsafe { app.run() }
}
