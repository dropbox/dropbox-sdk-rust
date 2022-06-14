from contextlib import contextmanager

from stone import ir
from stone.backend import CodeBackend
from stone.backends.helpers import (
    fmt_pascal,
    fmt_underscores
)

RUST_RESERVED_WORDS = [
    "abstract", "alignof", "as", "async", "become", "box", "break", "const", "continue", "crate",
    "do", "else", "enum", "extern", "false", "final", "fn", "for", "if", "impl", "in", "let",
    "loop", "macro", "match", "mod", "move", "mut", "offsetof", "override", "priv", "proc", "pub",
    "pure", "ref", "return", "Self", "self", "sizeof", "static", "struct", "super", "trait",
    "true", "type", "typeof", "unsafe", "unsized", "use", "virtual", "where", "while", "yield",
]

# Also avoid using names of types that are in the prelude for the names of our types.
RUST_GLOBAL_NAMESPACE = [
    "Copy", "Send", "Sized", "Sync", "Drop", "Fn", "FnMut", "FnOnce", "drop", "Box", "ToOwned",
    "Clone", "PartialEq", "PartialOrd", "Eq", "Ord", "AsRef", "AsMut", "Into", "From", "Default",
    "Iterator", "Extend", "IntoIterator", "DoubleEndedIterator", "ExactSizeIterator", "Option",
    "Some", "None", "Result", "Ok", "Err", "SliceConcatExt", "String", "ToString", "Vec",
]

# These namespaces contain types used in the core SDK code and must always be compiled in.
REQUIRED_NAMESPACES = ["auth"]

# Additional types we want to implement Display for. Outside of this list, only error-like types
# get a Display impl.
EXTRA_DISPLAY_TYPES = ["auth::RateLimitReason"]


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
            arg_list += (u', ' if arg_list != u'' else u'') + arg
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

    def is_enum_type(self, typ):
        return isinstance(typ, ir.Union) or \
            (isinstance(typ, ir.Struct) and typ.has_enumerated_subtypes())

    def is_nullary_struct(self, typ):
        return isinstance(typ, ir.Struct) and not typ.all_fields

    def is_closed_union(self, typ):
        return (isinstance(typ, ir.Union) and typ.closed) \
            or (isinstance(typ, ir.Struct) \
                and typ.has_enumerated_subtypes() and not typ.is_catch_all())

    def get_enum_variants(self, typ):
        if isinstance(typ, ir.Union):
            return typ.all_fields
        elif isinstance(typ, ir.Struct) and typ.has_enumerated_subtypes():
            return typ.get_enumerated_subtypes()
        else:
            return []

    def namespace_name(self, ns):
        return self.namespace_name_raw(ns.name)

    def namespace_name_raw(self, ns_name):
        name = fmt_underscores(ns_name)
        if name in RUST_RESERVED_WORDS + RUST_GLOBAL_NAMESPACE:
            name = 'dbx_' + name
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
        return self.route_name_raw(route.name, route.version)

    def route_name_raw(self, name, version):
        name = fmt_underscores(name)
        if version > 1:
            name = '{}_v{}'.format(name, version)
        if name in RUST_RESERVED_WORDS:
            name = 'do_' + name
        return name

    def alias_name(self, alias):
        name = fmt_pascal(alias.name)
        if name in RUST_RESERVED_WORDS + RUST_GLOBAL_NAMESPACE:
            name += 'Alias'
        return name

    def rust_type(self, typ, current_namespace, no_qualify=False, crate='crate'):
        if isinstance(typ, ir.Nullable):
            return u'Option<{}>'.format(self.rust_type(typ.data_type, current_namespace, no_qualify, crate))
        elif isinstance(typ, ir.Void):
            return u'()'
        elif isinstance(typ, ir.Bytes):
            return u'Vec<u8>'
        elif isinstance(typ, ir.Int32):
            return u'i32'
        elif isinstance(typ, ir.UInt32):
            return u'u32'
        elif isinstance(typ, ir.Int64):
            return u'i64'
        elif isinstance(typ, ir.UInt64):
            return u'u64'
        elif isinstance(typ, ir.Float32):
            return u'f32'
        elif isinstance(typ, ir.Float64):
            return u'f64'
        elif isinstance(typ, ir.Boolean):
            return u'bool'
        elif isinstance(typ, ir.String):
            return u'String'
        elif isinstance(typ, ir.Timestamp):
            return u'String /*Timestamp*/'  # TODO
        elif isinstance(typ, ir.List):
            return u'Vec<{}>'.format(self.rust_type(typ.data_type, current_namespace, no_qualify, crate))
        elif isinstance(typ, ir.Map):
            return u'::std::collections::HashMap<{}, {}>'.format(
                self.rust_type(typ.key_data_type, current_namespace, no_qualify, crate),
                self.rust_type(typ.value_data_type, current_namespace, no_qualify, crate))
        elif isinstance(typ, ir.Alias):
            if typ.namespace.name == current_namespace or no_qualify:
                return self.alias_name(typ)
            else:
                return u'{}::{}::{}'.format(
                    crate,
                    self.namespace_name(typ.namespace),
                    self.alias_name(typ))
        elif isinstance(typ, ir.UserDefined):
            if isinstance(typ, ir.Struct):
                name = self.struct_name(typ)
            elif isinstance(typ, ir.Union):
                name = self.enum_name(typ)
            else:
                raise RuntimeError(u'ERROR: user-defined type "{}" is neither Struct nor Union???'
                                   .format(typ))
            if typ.namespace.name == current_namespace or no_qualify:
                return name
            else:
                return u'{}::{}::{}'.format(
                    crate,
                    self.namespace_name(typ.namespace),
                    name)
        else:
            raise RuntimeError(u'ERROR: unhandled type "{}"'.format(typ))
