from abc import ABC
from contextlib import contextmanager
from typing import Optional

from stone import ir
from stone.backend import CodeBackend
from stone.backends.helpers import (
    fmt_pascal,
    fmt_underscores
)
from stone.ir import Alias, ApiNamespace, ApiRoute, DataType, Field, Struct, StructField

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


def _arg_list(args: list[str]) -> str:
    arg_list = ''
    for arg in args:
        arg_list += (', ' if arg_list != '' else '') + arg
    return arg_list


class RustHelperBackend(CodeBackend, ABC):
    """
    A superclass for RustGenerator and TestGenerator to contain some common rust-generation methods.
    """

    def _dent_len(self) -> int:
        if self.tabs_for_indents:
            return 4 * self.cur_indent
        else:
            return self.cur_indent

    @contextmanager
    def emit_rust_function_def(
            self,
            name: str,
            args: Optional[list[str]] = None,
            return_type: Optional[str] = None,
            access: Optional[str] = None,
    ):
        """
        A Rust function definition context manager.
        """
        if args is None:
            args = []
        if access is None:
            access = ''
        else:
            access += ' '
        ret = f' -> {return_type}' if return_type is not None else ''
        one_line = f'{access}fn {name}({_arg_list(args)}){ret} {{'
        if self._dent_len() + len(one_line) < 100:
            # one-line version
            self.emit(one_line)
        else:
            # one arg per line
            self.emit(f'{access}fn {name}(')
            with self.indent():
                for arg in args:
                    self.emit(arg + ',')
            self.emit(f'){ret} {{')

        with self.indent():
            yield
        self.emit('}')

    def emit_rust_fn_call(self, func_name: str, args: list[str], end: Optional[str] = None) -> None:
        """
        Emit a Rust function call. Wraps arguments to multiple lines if it gets too long.
        If `end` is None, the call ends without any semicolon.
        """
        if end is None:
            end = ''
        one_line = f'{func_name}({_arg_list(args)}){end}'
        if self._dent_len() + len(one_line) < 100:
            self.emit(one_line)
        else:
            self.emit(func_name + '(')
            with self.indent():
                for i, arg in enumerate(args):
                    self.emit(arg + (',' if i + 1 < len(args) else (')' + end)))

    def is_enum_type(self, typ: DataType) -> bool:
        return isinstance(typ, ir.Union) or \
            (isinstance(typ, ir.Struct) and typ.has_enumerated_subtypes())

    def is_nullary_struct(self, typ: DataType) -> bool:
        return isinstance(typ, ir.Struct) and not typ.all_fields

    def is_closed_union(self, typ: DataType) -> bool:
        return (isinstance(typ, ir.Union) and typ.closed) \
            or (isinstance(typ, ir.Struct)
                and typ.has_enumerated_subtypes() and not typ.is_catch_all())

    def get_enum_variants(self, typ: DataType) -> list[StructField]:
        if isinstance(typ, ir.Union):
            return typ.all_fields
        elif isinstance(typ, ir.Struct) and typ.has_enumerated_subtypes():
            return typ.get_enumerated_subtypes()
        else:
            return []

    def namespace_name(self, ns: ApiNamespace) -> str:
        return self.namespace_name_raw(ns.name)

    def namespace_name_raw(self, ns_name: str) -> str:
        name = fmt_underscores(ns_name)
        if name in RUST_RESERVED_WORDS + RUST_GLOBAL_NAMESPACE:
            name = 'dbx_' + name
        return name

    def struct_name(self, struct: Struct) -> str:
        name = fmt_pascal(struct.name)
        if name in RUST_RESERVED_WORDS + RUST_GLOBAL_NAMESPACE:
            name += 'Struct'
        return name

    def enum_name(self, union: DataType) -> str:
        name = fmt_pascal(union.name)
        if name in RUST_RESERVED_WORDS + RUST_GLOBAL_NAMESPACE:
            name += 'Union'
        return name

    def field_name(self, field: StructField) -> str:
        return self.field_name_raw(field.name)

    def field_name_raw(self, name: str) -> str:
        name = fmt_underscores(name)
        if name in RUST_RESERVED_WORDS:
            name += '_field'
        return name

    def enum_variant_name(self, field: StructField) -> str:
        return self.enum_variant_name_raw(field.name)

    def enum_variant_name_raw(self, name: str) -> str:
        name = fmt_pascal(name)
        if name in RUST_RESERVED_WORDS:
            name += 'Variant'
        return name

    def route_name(self, route: ApiRoute) -> str:
        return self.route_name_raw(route.name, route.version)

    def route_name_raw(self, name: str, version: int) -> str:
        name = fmt_underscores(name)
        if version > 1:
            name = f'{name}_v{version}'
        if name in RUST_RESERVED_WORDS:
            name = 'do_' + name
        return name

    def alias_name(self, alias: Alias) -> str:
        name = fmt_pascal(alias.name)
        if name in RUST_RESERVED_WORDS + RUST_GLOBAL_NAMESPACE:
            name += 'Alias'
        return name

    def rust_type(self, typ: DataType, current_namespace: str, no_qualify=False, crate='crate') -> str:
        if isinstance(typ, ir.Nullable):
            t = self.rust_type(typ.data_type, current_namespace, no_qualify, crate)
            return f'Option<{t}>'
        elif isinstance(typ, ir.Void):
            return '()'
        elif isinstance(typ, ir.Bytes):
            return 'Vec<u8>'
        elif isinstance(typ, ir.Int32):
            return 'i32'
        elif isinstance(typ, ir.UInt32):
            return 'u32'
        elif isinstance(typ, ir.Int64):
            return 'i64'
        elif isinstance(typ, ir.UInt64):
            return 'u64'
        elif isinstance(typ, ir.Float32):
            return 'f32'
        elif isinstance(typ, ir.Float64):
            return 'f64'
        elif isinstance(typ, ir.Boolean):
            return 'bool'
        elif isinstance(typ, ir.String):
            return 'String'
        elif isinstance(typ, ir.Timestamp):
            return 'String /*Timestamp*/'  # TODO
        elif isinstance(typ, ir.List):
            t = self.rust_type(typ.data_type, current_namespace, no_qualify, crate)
            return f'Vec<{t}>'
        elif isinstance(typ, ir.Map):
            k = self.rust_type(typ.key_data_type, current_namespace, no_qualify, crate)
            v = self.rust_type(typ.value_data_type, current_namespace, no_qualify, crate)
            return f'::std::collections::HashMap<{k}, {v}>'
        elif isinstance(typ, ir.Alias):
            if typ.namespace.name == current_namespace or no_qualify:
                return self.alias_name(typ)
            else:
                return f'{crate}::{self.namespace_name(typ.namespace)}::{self.alias_name(typ)}'
        elif isinstance(typ, ir.UserDefined):
            if isinstance(typ, ir.Struct):
                name = self.struct_name(typ)
            elif isinstance(typ, ir.Union):
                name = self.enum_name(typ)
            else:
                raise RuntimeError(f'ERROR: user-defined type "{typ}" is neither Struct nor Union???')
            if typ.namespace.name == current_namespace or no_qualify:
                return name
            else:
                return f'{crate}::{self.namespace_name(typ.namespace)}::{name}'
        else:
            raise RuntimeError(f'ERROR: unhandled type "{typ}"')
