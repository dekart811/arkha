//! A fix for GTK 3's accessibility.
//!
//! Asked for its state once its widget is gone, the accessible object of a
//! window, button, toggle button (the path bar's, rebuilt on every change
//! of folder), label, entry, expander, check menu item or icon view item
//! gives nothing, where it should say it's defunct. The accessibility
//! bridge then fails two assertions for each such object, every time a
//! client (a screen reader) reads the tree. Their classes' ref_state_set is
//! wrapped here to give a defunct state instead; everything else is left to
//! GTK.
//!
//! A notebook's tab (its page accessible) is worse off: removed (a tab
//! closed, the extra pane of split view closed), it forgets its notebook,
//! and asked for its state then, it passes the missing notebook to
//! atk_selection_ref_selection and gtk_accessible_get_widget, for its state,
//! name and index in its parent: failed assertions. Those three are wrapped
//! to answer (defunct, no name, -1) without asking it once it has no parent
//! (which GTK takes away on removal).

use gtk::atk::ffi as atk;

type RefStateSet = unsafe extern "C" fn(*mut atk::AtkObject) -> *mut atk::AtkStateSet;

/// GTK's own functions of the wrapped classes, in the order
/// fix_defunct_states wraps them.
static ORIGINALS: [std::sync::OnceLock<RefStateSet>; 8] = [const { std::sync::OnceLock::new() }; 8];

unsafe extern "C" {
    fn gtk_window_accessible_get_type() -> glib::ffi::GType;
    fn gtk_button_accessible_get_type() -> glib::ffi::GType;
    fn gtk_toggle_button_accessible_get_type() -> glib::ffi::GType;
    fn gtk_label_accessible_get_type() -> glib::ffi::GType;
    fn gtk_entry_accessible_get_type() -> glib::ffi::GType;
    fn gtk_expander_accessible_get_type() -> glib::ffi::GType;
    fn gtk_check_menu_item_accessible_get_type() -> glib::ffi::GType;
    fn gtk_notebook_page_accessible_get_type() -> glib::ffi::GType;
}

/// A state set saying the object is gone.
unsafe fn defunct() -> *mut atk::AtkStateSet {
    // SAFETY: plain ATK calls.
    unsafe {
        let set = atk::atk_state_set_new();
        atk::atk_state_set_add_state(set, atk::ATK_STATE_DEFUNCT);
        set
    }
}

/// GTK's ref_state_set, get_name and get_index_in_parent of a notebook's tab.
static TAB_ORIGINAL: std::sync::OnceLock<RefStateSet> = std::sync::OnceLock::new();
type GetName = unsafe extern "C" fn(*mut atk::AtkObject) -> *const std::ffi::c_char;
static TAB_NAME: std::sync::OnceLock<GetName> = std::sync::OnceLock::new();
type GetIndex = unsafe extern "C" fn(*mut atk::AtkObject) -> std::ffi::c_int;
static TAB_INDEX: std::sync::OnceLock<GetIndex> = std::sync::OnceLock::new();

/// Whether a tab is removed: GTK took its parent (its notebook) away.
unsafe fn tab_removed(obj: *mut atk::AtkObject) -> bool {
    // SAFETY: an ATK object, from ATK.
    unsafe { atk::atk_object_get_parent(obj).is_null() }
}

unsafe extern "C" fn tab_get_name(obj: *mut atk::AtkObject) -> *const std::ffi::c_char {
    // SAFETY: as in tab_ref_state_set.
    unsafe {
        match TAB_NAME.get() {
            Some(f) if !tab_removed(obj) => f(obj),
            _ => std::ptr::null(),
        }
    }
}

unsafe extern "C" fn tab_get_index_in_parent(obj: *mut atk::AtkObject) -> std::ffi::c_int {
    // SAFETY: as in tab_ref_state_set.
    unsafe {
        match TAB_INDEX.get() {
            Some(f) if !tab_removed(obj) => f(obj),
            _ => -1,
        }
    }
}

/// A tab's state: defunct once it's removed (no parent), without asking it.
unsafe extern "C" fn tab_ref_state_set(obj: *mut atk::AtkObject) -> *mut atk::AtkStateSet {
    // SAFETY: called by ATK with one of its tab objects, through the slot
    // whose original function was kept before it was replaced.
    unsafe {
        if tab_removed(obj) {
            return defunct();
        }
        match TAB_ORIGINAL.get() {
            Some(f) => f(obj),
            None => defunct(),
        }
    }
}

/// One for each class (a class's function may call its parent's, so each
/// has to know whose original to call).
unsafe extern "C" fn ref_state_set<const N: usize>(obj: *mut atk::AtkObject) -> *mut atk::AtkStateSet {
    // SAFETY: called by ATK with one of its objects, through class N's
    // slot, whose original function was kept before it was replaced.
    unsafe {
        let set = ORIGINALS[N].get().map_or(std::ptr::null_mut(), |f| f(obj));
        if !set.is_null() {
            return set;
        }
        defunct()
    }
}

const WRAPPERS: [RefStateSet; 8] = [
    ref_state_set::<0>, ref_state_set::<1>, ref_state_set::<2>, ref_state_set::<3>,
    ref_state_set::<4>, ref_state_set::<5>, ref_state_set::<6>, ref_state_set::<7>,
];

/// Wraps the classes' ref_state_set (once, at start: before any of their
/// subclasses is set up, which then copy the wrapped function). Parents
/// come before their subclasses.
pub fn fix_defunct_states() {
    let mut types = Vec::new();
    // SAFETY: GTK's own type functions (public API); they register the types.
    unsafe {
        types.extend([
            gtk_window_accessible_get_type(),
            gtk_button_accessible_get_type(),
            gtk_toggle_button_accessible_get_type(),
            gtk_label_accessible_get_type(),
            gtk_entry_accessible_get_type(),
            gtk_expander_accessible_get_type(),
            gtk_check_menu_item_accessible_get_type(),
        ]);
    }
    // The icon view's item class is private: registered when the first item
    // is made, so one is.
    let store = gtk::ListStore::new(&[glib::types::Type::STRING]);
    store.insert_with_values(None, &[(0, &"")]);
    let view = gtk::IconView::with_model(&store);
    view.set_text_column(0);
    use gtk::prelude::*;
    // SAFETY: plain GObject and ATK calls on objects that live through them.
    unsafe {
        if let Some(acc) = view.accessible() {
            let child = atk::atk_object_ref_accessible_child(glib::translate::ToGlibPtr::to_glib_none(&acc).0, 0);
            if !child.is_null() {
                glib::gobject_ffi::g_object_unref(child as *mut _);
            }
        }
        let item = glib::gobject_ffi::g_type_from_name(c"GtkIconViewItemAccessible".as_ptr());
        if item != 0 {
            types.push(item);
        }
    }
    // SAFETY: as for the others (it has no subclasses).
    unsafe {
        let class = glib::gobject_ffi::g_type_class_ref(gtk_notebook_page_accessible_get_type()) as *mut atk::AtkObjectClass;
        if let Some(f) = (*class).ref_state_set {
            if TAB_ORIGINAL.set(f).is_ok() {
                (*class).ref_state_set = Some(tab_ref_state_set);
            }
        }
        if let Some(f) = (*class).get_name {
            if TAB_NAME.set(f).is_ok() {
                (*class).get_name = Some(tab_get_name);
            }
        }
        if let Some(f) = (*class).get_index_in_parent {
            if TAB_INDEX.set(f).is_ok() {
                (*class).get_index_in_parent = Some(tab_get_index_in_parent);
            }
        }
    }
    for (n, ty) in types.into_iter().enumerate() {
        if ORIGINALS[n].get().is_some() {
            continue;
        }
        // SAFETY: the class is kept referenced for good (g_type_class_ref),
        // so the wrapped function stays in place; it's an AtkObject class.
        unsafe {
            let class = glib::gobject_ffi::g_type_class_ref(ty) as *mut atk::AtkObjectClass;
            if let Some(f) = (*class).ref_state_set {
                if ORIGINALS[n].set(f).is_ok() {
                    (*class).ref_state_set = Some(WRAPPERS[n]);
                }
            }
        }
    }
}
