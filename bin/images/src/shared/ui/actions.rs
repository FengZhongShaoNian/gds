//! Workspace keyboard actions and context-scoped keybindings.

use gpui_kit::{Action, App, KeyBinding, KeyBindingContextPredicate, actions};

pub const WORKSPACE_CONTEXT: &str = "ImagesWorkspace";
pub const CANVAS_CONTEXT: &str = "ImagesCanvas";
pub const SETTINGS_CONTEXT: &str = "ImagesSettings";

const WORKSPACE_NON_INPUT: &str = "ImagesWorkspace && !Input && !Dialog";
const WORKSPACE_NO_DIALOG: &str = "ImagesWorkspace && !Dialog";
const SETTINGS_NON_INPUT: &str = "ImagesSettings && !Input && !Dialog";

actions!(
    gds_images,
    [
        OpenFile,
        OpenClipboard,
        OpenSettings,
        CloseSettings,
        PrevImage,
        NextImage,
        FirstImage,
        LastImage,
        ZoomIn,
        ZoomOut,
        ZoomFit,
        ZoomActual,
        RotateCw,
        RotateCcw,
        FlipHorizontal,
        FlipVertical,
        UndoEdit,
        RedoEdit,
        SaveImage,
        SaveImageAs,
        CopyImage,
        ToggleFilmstrip,
        ToggleInspector,
        EditLocation,
        RevealInFiles,
        OpenWithApp,
        MoveToTrash,
        DeleteCanvasObject,
        DuplicateAnnotations,
        SelectAllAnnotations,
        ConfirmCanvasAction,
        BringForward,
        SendBackward,
        BringToFront,
        SendToBack,
        NudgeLeft,
        NudgeRight,
        NudgeUp,
        NudgeDown,
        NudgeFastLeft,
        NudgeFastRight,
        NudgeFastUp,
        NudgeFastDown,
        SelectToolPointer,
        SelectToolPan,
        SelectToolCrop,
        SelectToolRect,
        SelectToolEllipse,
        SelectToolArrow,
        SelectToolLine,
        SelectToolText,
        SelectToolMosaic,
        SelectToolBlur,
        CancelInteraction,
        QuitApp,
    ]
);

/// Registers default keybindings for both the global workspace and non-input canvas focus.
pub fn bind_defaults(cx: &mut App) {
    cx.bind_keys([
        scoped_binding("ctrl-o", OpenFile, WORKSPACE_NO_DIALOG, cx),
        scoped_binding("ctrl-shift-v", OpenClipboard, WORKSPACE_NON_INPUT, cx),
        scoped_binding("ctrl-s", SaveImage, WORKSPACE_NO_DIALOG, cx),
        scoped_binding("ctrl-shift-s", SaveImageAs, WORKSPACE_NO_DIALOG, cx),
        scoped_binding("ctrl-c", CopyImage, WORKSPACE_NON_INPUT, cx),
        scoped_binding("ctrl-z", UndoEdit, WORKSPACE_NON_INPUT, cx),
        scoped_binding("ctrl-shift-z", RedoEdit, WORKSPACE_NON_INPUT, cx),
        scoped_binding("ctrl-y", RedoEdit, WORKSPACE_NON_INPUT, cx),
        scoped_binding("ctrl-=", ZoomIn, WORKSPACE_NO_DIALOG, cx),
        scoped_binding("ctrl--", ZoomOut, WORKSPACE_NO_DIALOG, cx),
        scoped_binding("ctrl-0", ZoomFit, WORKSPACE_NO_DIALOG, cx),
        scoped_binding("ctrl-1", ZoomActual, WORKSPACE_NO_DIALOG, cx),
        scoped_binding("ctrl-b", ToggleFilmstrip, WORKSPACE_NO_DIALOG, cx),
        scoped_binding("ctrl-l", EditLocation, WORKSPACE_NO_DIALOG, cx),
        scoped_binding("ctrl-i", ToggleInspector, WORKSPACE_NO_DIALOG, cx),
        scoped_binding("ctrl-delete", MoveToTrash, WORKSPACE_NON_INPUT, cx),
        KeyBinding::new("ctrl-,", OpenSettings, Some(WORKSPACE_CONTEXT)),
        KeyBinding::new("ctrl-q", QuitApp, Some(WORKSPACE_CONTEXT)),
        scoped_binding("escape", CancelInteraction, WORKSPACE_NON_INPUT, cx),
        KeyBinding::new("ctrl-,", CloseSettings, Some(SETTINGS_CONTEXT)),
        scoped_binding("escape", CloseSettings, SETTINGS_NON_INPUT, cx),
        scoped_binding("alt-left", CloseSettings, SETTINGS_NON_INPUT, cx),
        KeyBinding::new("ctrl-b", ToggleFilmstrip, Some(SETTINGS_CONTEXT)),
        KeyBinding::new("ctrl-q", QuitApp, Some(SETTINGS_CONTEXT)),
        // Canvas-only shortcuts so text inputs never lose keystrokes.
        KeyBinding::new("left", NudgeLeft, Some(CANVAS_CONTEXT)),
        KeyBinding::new("up", NudgeUp, Some(CANVAS_CONTEXT)),
        KeyBinding::new("right", NudgeRight, Some(CANVAS_CONTEXT)),
        KeyBinding::new("down", NudgeDown, Some(CANVAS_CONTEXT)),
        KeyBinding::new("shift-left", NudgeFastLeft, Some(CANVAS_CONTEXT)),
        KeyBinding::new("shift-up", NudgeFastUp, Some(CANVAS_CONTEXT)),
        KeyBinding::new("shift-right", NudgeFastRight, Some(CANVAS_CONTEXT)),
        KeyBinding::new("shift-down", NudgeFastDown, Some(CANVAS_CONTEXT)),
        KeyBinding::new("home", FirstImage, Some(CANVAS_CONTEXT)),
        KeyBinding::new("end", LastImage, Some(CANVAS_CONTEXT)),
        KeyBinding::new("+", ZoomIn, Some(CANVAS_CONTEXT)),
        KeyBinding::new("-", ZoomOut, Some(CANVAS_CONTEXT)),
        KeyBinding::new("f", ZoomFit, Some(CANVAS_CONTEXT)),
        KeyBinding::new("r", RotateCw, Some(CANVAS_CONTEXT)),
        KeyBinding::new("shift-r", RotateCcw, Some(CANVAS_CONTEXT)),
        KeyBinding::new("h", FlipHorizontal, Some(CANVAS_CONTEXT)),
        KeyBinding::new("v", FlipVertical, Some(CANVAS_CONTEXT)),
        KeyBinding::new("i", ToggleInspector, Some(CANVAS_CONTEXT)),
        KeyBinding::new("delete", DeleteCanvasObject, Some(CANVAS_CONTEXT)),
        KeyBinding::new("backspace", DeleteCanvasObject, Some(CANVAS_CONTEXT)),
        KeyBinding::new("ctrl-d", DuplicateAnnotations, Some(CANVAS_CONTEXT)),
        KeyBinding::new("ctrl-a", SelectAllAnnotations, Some(CANVAS_CONTEXT)),
        KeyBinding::new("enter", ConfirmCanvasAction, Some(CANVAS_CONTEXT)),
        KeyBinding::new("[", SendBackward, Some(CANVAS_CONTEXT)),
        KeyBinding::new("]", BringForward, Some(CANVAS_CONTEXT)),
        KeyBinding::new("shift-[", SendToBack, Some(CANVAS_CONTEXT)),
        KeyBinding::new("shift-]", BringToFront, Some(CANVAS_CONTEXT)),
        KeyBinding::new("ctrl-[", SendToBack, Some(CANVAS_CONTEXT)),
        KeyBinding::new("ctrl-]", BringToFront, Some(CANVAS_CONTEXT)),
        KeyBinding::new("1", SelectToolPointer, Some(CANVAS_CONTEXT)),
        KeyBinding::new("2", SelectToolPan, Some(CANVAS_CONTEXT)),
        KeyBinding::new("3", SelectToolCrop, Some(CANVAS_CONTEXT)),
        KeyBinding::new("4", SelectToolRect, Some(CANVAS_CONTEXT)),
        KeyBinding::new("5", SelectToolEllipse, Some(CANVAS_CONTEXT)),
        KeyBinding::new("6", SelectToolArrow, Some(CANVAS_CONTEXT)),
        KeyBinding::new("7", SelectToolLine, Some(CANVAS_CONTEXT)),
        KeyBinding::new("8", SelectToolText, Some(CANVAS_CONTEXT)),
        KeyBinding::new("9", SelectToolMosaic, Some(CANVAS_CONTEXT)),
        KeyBinding::new("0", SelectToolBlur, Some(CANVAS_CONTEXT)),
        // Keep popup menus winning over workspace-level Escape and Enter actions.
        scoped_binding("escape", gpui_kit::base::actions::Cancel, "PopupMenu", cx),
        scoped_binding(
            "enter",
            gpui_kit::base::actions::Confirm { secondary: false },
            "PopupMenu",
            cx,
        ),
    ]);
}

fn scoped_binding(stroke: &str, action: impl Action, context: &str, cx: &App) -> KeyBinding {
    let predicate =
        KeyBindingContextPredicate::parse(context).expect("bundled action contexts are valid");
    KeyBinding::load(
        stroke,
        action.boxed_clone(),
        Some(predicate.into()),
        false,
        None,
        cx,
    )
    .expect("bundled action shortcuts are valid")
}
