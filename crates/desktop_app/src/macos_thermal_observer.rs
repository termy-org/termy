#[cfg(target_os = "macos")]
pub(crate) fn neutralize_gpui_thermal_state_observer() {
    use cocoa::base::{id, nil};
    use cocoa::foundation::NSString;
    use objc::runtime::{
        Class, Imp, Method, Object, Sel, class_getInstanceMethod, method_setImplementation,
    };
    use objc::{class, msg_send, sel, sel_impl};
    use std::ptr;

    extern "C" fn ignore_thermal_state_change(_this: &mut Object, _sel: Sel, _notification: id) {}

    unsafe {
        let app: id = msg_send![class!(NSApplication), sharedApplication];
        let delegate: id = msg_send![app, delegate];
        if delegate == nil {
            return;
        }

        // If the observer is still registered, replacing the selector with a no-op keeps
        // Apple's notification delivery from re-entering GPUI's App RefCell.
        let delegate_class: *mut Class = msg_send![delegate, class];
        if !delegate_class.is_null() {
            let method = class_getInstanceMethod(delegate_class, sel!(onThermalStateChange:));
            if !method.is_null() {
                let replacement: Imp = std::mem::transmute(
                    ignore_thermal_state_change as extern "C" fn(&mut Object, Sel, id),
                );
                method_setImplementation(method as *mut Method, replacement);
            }
        }

        let notification_center: id = msg_send![class!(NSNotificationCenter), defaultCenter];
        let process_info: id = msg_send![class!(NSProcessInfo), processInfo];
        let notification_name =
            NSString::alloc(nil).init_str("NSProcessInfoThermalStateDidChangeNotification");

        // This GPUI revision invokes the thermal callback directly from Apple's
        // notification queue, which can re-enter App while it is already borrowed.
        let _: () = msg_send![
            notification_center,
            removeObserver: delegate
            name: notification_name
            object: process_info
        ];
        let _: () = msg_send![
            notification_center,
            removeObserver: delegate
            name: notification_name
            object: ptr::null::<Object>() as id
        ];
        let _: () = msg_send![notification_name, release];
    }
}
