/// Translate with the active catalogue; `@context` selects an explicit [`Gds`](crate::app::Gds).
///
/// Arguments are Fluent variables. A bare identifier is shorthand for a
/// variable with the same name; `name = expression` supplies an explicit value.
/// Both forms can be mixed and accept a trailing comma.
#[macro_export]
macro_rules! t {
    (@$cx:expr, $id:literal $(,)?) => {
        $cx.i18n().text($id)
    };
    (@$cx:expr, $id:literal, $($name:ident $(= $value:expr)?),+ $(,)?) => {{
        let mut arguments = $crate::i18n::FluentArgs::new();
        $(
            arguments.set(
                ::core::stringify!($name),
                $crate::t!(@argument $name $(= $value)?),
            );
        )+
        $cx.i18n().format($id, ::core::option::Option::Some(&arguments))
    }};
    ($id:literal $(, $name:ident $(= $value:expr)?)* $(,)?) => {
        $crate::t!(@$crate::app::Gds::current(), $id $(, $name $(= $value)?)*)
    };
    (@argument $name:ident = $value:expr) => {
        $value
    };
    (@argument $name:ident) => {
        $name
    };
}
