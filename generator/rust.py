from contextlib import contextmanager

from stone.backend import CodeBackend
from stone.backends.helpers import (
    fmt_pascal,
    fmt_underscores
)

RUST_RESERVED_WORDS = [
    "abstract", "alignof", "as", "become", "box", "break", "const", "continue", "crate", "do",
    "else", "enum", "extern", "false", "final", "fn", "for", "if", "impl", "in", "let", "loop",
    "macro", "match", "mod", "move", "mut", "offsetof", "override", "priv", "proc", "pub", "pure",
    "ref", "return", "Self", "self", "sizeof", "static", "struct", "super", "trait", "true", "type",
    "typeof", "unsafe", "unsized", "use", "virtual", "where", "while", "yield",
]

# Also avoid using names of types that are in the prelude for the names of our types.
RUST_GLOBAL_NAMESPACE = [
    "Copy", "Send", "Sized", "Sync", "Drop", "Fn", "FnMut", "FnOnce", "drop", "Box", "ToOwned",
    "Clone", "PartialEq", "PartialOrd", "Eq", "Ord", "AsRef", "AsMut", "Into", "From", "Default",
    "Iterator", "Extend", "IntoIterator", "DoubleEndedIterator", "ExactSizeIterator", "Option",
    "Some", "None", "Result", "Ok", "Err", "SliceConcatExt", "String", "ToString", "Vec",
]


class RustHelperBackend(CodeBackend):
    """
    A superclass for RustGenerator and TestGenerator to contain some common rust-generation methods.
    """

    def _dent_len(self):
        if self.tabs_for_indents:
            return 4 * self.cur_indent
        else:
            return self.cur_indent

    def _arg_list(self, args):
        arg_list = u''
        for arg in args:
            arg_list += (u', ' if arg_list is not u'' else u'') + arg
        return arg_list

    @contextmanager
    def emit_rust_function_def(self, name, args=[], return_type=None, access=None):
        """
        A Rust function definition context manager.
        """
        if access is None:
            access = u''
        else:
            access += u' '
        ret = u' -> {}'.format(return_type) if return_type is not None else u''
        one_line = u'{}fn {}({}){} {{'.format(
            access,
            name,
            self._arg_list(args),
            ret)
        if self._dent_len() + len(one_line) < 100:
            # one-line version
            self.emit(one_line)
        else:
            # one arg per line
            self.emit(u'{}fn {}('.format(access, name))
            with self.indent():
                for arg in args:
                    self.emit(arg + ',')
            self.emit(u'){} {{'.format(ret))

        with self.indent():
            yield
        self.emit(u'}')

    def emit_rust_fn_call(self, func_name, args, end=None):
        """
        Emit a Rust function call. Wraps arguments to multiple lines if it gets too long.
        If `end` is None, the call ends without any semicolon.
        """
        if end is None:
            end = u''
        one_line = u'{}({}){}'.format(
            func_name,
            self._arg_list(args),
            end)
        if self._dent_len() + len(one_line) < 100:
            self.emit(one_line)
        else:
            self.emit(func_name + u'(')
            with self.indent():
                for i, arg in enumerate(args):
                    self.emit(arg + (',' if i+1 < len(args) else (')' + end)))

    def namespace_name(self, ns):
        name = fmt_underscores(ns.name)
        if name in RUST_RESERVED_WORDS + RUST_GLOBAL_NAMESPACE:
            name += '_namespace'
        return name

    def struct_name(self, struct):
        name = fmt_pascal(struct.name)
        if name in RUST_RESERVED_WORDS + RUST_GLOBAL_NAMESPACE:
            name += 'Struct'
        return name

    def enum_name(self, union):
        name = fmt_pascal(union.name)
        if name in RUST_RESERVED_WORDS + RUST_GLOBAL_NAMESPACE:
            name += 'Union'
        return name

    def field_name(self, field):
        return self.field_name_raw(field.name)

    def field_name_raw(self, name):
        name = fmt_underscores(name)
        if name in RUST_RESERVED_WORDS:
            name += '_field'
        return name

    def enum_variant_name(self, field):
        return self.enum_variant_name_raw(field.name)

    def enum_variant_name_raw(self, name):
        name = fmt_pascal(name)
        if name in RUST_RESERVED_WORDS:
            name += 'Variant'
        return name

    def route_name(self, route):
        name = fmt_underscores(route.name)
        if name in RUST_RESERVED_WORDS:
            name = 'do_' + name
        return name

    def alias_name(self, alias):
        name = fmt_pascal(alias.name)
        if name in RUST_RESERVED_WORDS + RUST_GLOBAL_NAMESPACE:
            name += 'Alias'
        return name
