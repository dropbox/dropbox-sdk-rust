import contextlib
from contextlib import contextmanager
from typing import Iterator, Optional, Sequence

from rust import RustHelperBackend, EXTRA_DISPLAY_TYPES, REQUIRED_NAMESPACES
from stone import ir
from stone.backends.helpers import split_words


DERIVE_TRAITS = ['Debug', 'Clone', 'PartialEq']


def fmt_shouting_snake(name: str) -> str:
    return '_'.join([word.upper() for word in split_words(name)])


class RustBackend(RustHelperBackend):
    def __init__(self, target_folder_path: str, args: Optional[Sequence[str]]) -> None:
        super().__init__(target_folder_path, args)
        self.preserve_aliases = True

        self._all_types: dict[str, dict[str, ir.UserDefined]] = dict()
        self._current_namespace: str = ''
        self._error_types: set[Optional[ir.DataType]] = set()
        self._modules: list[str] = []

    # File Generators

    def generate(self, api: ir.Api) -> None:
        self._all_types = {ns.name: {typ.name: typ for typ in ns.data_types}
                           for ns in api.namespaces.values()}

        # All types used as an error for any route:
        self._error_types = set([
            route.error_data_type
            for ns in api.namespaces.values()
            for route in ns.routes
        ])
        # Also all enum types whose names end in 'Error'. These tend to be used as errors even when
        # not the direct error result from a route, i.e. they are inner members of other errors.
        self._error_types.update([
            typ
            for ns in api.namespaces.values()
            for typ in ns.data_types
            if self.is_enum_type(typ) and typ.name.endswith('Error')
        ])

        for namespace in api.namespaces.values():
            self._emit_namespace(namespace)

        for d in ['async_routes', 'sync_routes', 'types']:
            self._generate_mod_file(f'{d}/mod.rs')

        with self.output_to_relative_path('mod.rs'):
            self._emit_header()
            self.emit('pub mod types;')
            self.emit()
            with self.block('if_feature! { "async_routes",', delim=(None, '}')):
                self.emit('pub mod async_routes;')
            self.emit()
            with self.block('if_feature! { "sync_routes",', delim=(None, '}')):
                self.emit('pub mod sync_routes;')
            self.emit()
            with self.block('pub(crate) fn eat_json_fields<\'de, V>(map: &mut V)'
                            ' -> Result<(), V::Error>'
                            ' where V: ::serde::de::MapAccess<\'de>'):
                with self.block('while map.next_entry::<&str, ::serde_json::Value>()?.is_some()'):
                    self.emit('/* ignore */')
                self.emit('Ok(())')

    def _generate_mod_file(self, path: str) -> None:
        with self.output_to_relative_path(path):
            self._emit_header()
            self.emit('#![allow(missing_docs)]')
            self.emit()
            for module in self._modules:
                ns = self.namespace_name_raw(module)
                if module in REQUIRED_NAMESPACES:
                    self.emit(f'pub mod {ns};')
                else:
                    self.emit(f'if_feature! {{ "dbx_{module}", pub mod {ns}; }}')
                self.emit()

    # Type Emitters

    def _emit_namespace(self, namespace: ir.ApiNamespace) -> None:
        ns = self.namespace_name(namespace)
        self._current_namespace = namespace.name

        with self.output_to_relative_path(f'types/{ns}.rs'):
            self._emit_header()

            if namespace.doc is not None:
                self._emit_doc(namespace.doc, prefix='//!')
                self.emit()

            for alias in namespace.aliases:
                self._emit_alias(alias)
            if namespace.aliases:
                self.emit()

            for typ in namespace.data_types:
                self._current_type = typ
                if isinstance(typ, ir.Struct):
                    if typ.has_enumerated_subtypes():
                        self._emit_polymorphic_struct(typ)
                    else:
                        self._emit_struct(typ)
                elif isinstance(typ, ir.Union):
                    self._emit_union(typ)
                else:
                    raise RuntimeError(f'WARNING: unhandled type "{type(typ).__name__}" of field "{typ.name}"')

        with self.output_to_relative_path(f'sync_routes/{ns}.rs'):
            self._emit_header()
            self.emit('#[allow(unused_imports)]')
            self.emit(f'pub use crate::generated::types::{ns}::*;')
            self.emit()
            for fn in namespace.routes:
                self._emit_route(ns, fn)

        with self.output_to_relative_path(f'async_routes/{ns}.rs'):
            self._emit_header()
            self.emit('#[allow(unused_imports)]')
            self.emit(f'pub use crate::generated::types::{ns}::*;')
            self.emit()
            for fn in namespace.routes:
                self._emit_route(ns, fn, as_async=True)

        self._modules.append(namespace.name)

    def _emit_header(self) -> None:
        self.emit('// DO NOT EDIT')
        self.emit('// This file was @generated by Stone')
        self.emit()
        self.emit('#![allow(')
        self.emit('    clippy::too_many_arguments,')
        self.emit('    clippy::large_enum_variant,')
        self.emit('    clippy::result_large_err,')
        self.emit('    clippy::doc_markdown,')
        self.emit(')]')
        self.emit()

    def _emit_struct(self, struct: ir.Struct) -> None:
        struct_name = self.struct_name(struct)
        self._emit_doc(struct.doc)
        derive_traits = list(DERIVE_TRAITS)
        if self._can_derive_eq(struct):
            derive_traits.append('Eq')
        if not any(self._needs_explicit_default(field) for field in struct.all_fields):
            derive_traits.append('Default')
        self.emit(f'#[derive({", ".join(derive_traits)})]')
        self.emit('#[non_exhaustive] // structs may have more fields added in the future.')
        with self.block(f'pub struct {struct_name}'):
            for field in struct.all_fields:
                name = self.field_name(field)
                typ = self._rust_type(field.data_type)
                self._emit_doc(field.doc)
                self.emit(f'pub {name}: {typ},')
        self.emit()

        if not struct.all_required_fields and 'Default' not in derive_traits:
            self._impl_default_for_struct(struct)
            self.emit()

        if struct.all_required_fields or struct.all_optional_fields:
            with self._impl_struct(struct):
                self._emit_new_for_struct(struct)
            self.emit()

        self._impl_serde_for_struct(struct)

        if self._is_error_type(struct):
            self._impl_error(struct)
        elif self._rust_type(struct) in EXTRA_DISPLAY_TYPES:
            self._impl_display(struct)

        if struct.parent_type:
            if struct.parent_type.has_enumerated_subtypes():
                self._impl_from_for_polymorphic_struct(struct, struct.parent_type)
            else:
                self._impl_from_for_struct(struct, struct.parent_type)

    def _emit_polymorphic_struct(self, struct: ir.Struct) -> None:
        enum_name = self.enum_name(struct)
        self._emit_doc(struct.doc)
        derive_traits = list(DERIVE_TRAITS)
        if self._can_derive_eq(struct):
            derive_traits.append('Eq')
        self.emit(f'#[derive({", ".join(derive_traits)})]')
        if struct.is_catch_all():
            self.emit('#[non_exhaustive] // variants may be added in the future')
        with self.block(f'pub enum {enum_name}'):
            for subtype in struct.get_enumerated_subtypes():
                name = self.enum_variant_name(subtype)
                typ = self._rust_type(subtype.data_type)
                self.emit(f'{name}({typ}),')
            if struct.is_catch_all():
                self._emit_other_variant()
        self.emit()

        self._impl_serde_for_polymorphic_struct(struct)

    def _emit_union(self, union: ir.Union) -> None:
        enum_name = self.enum_name(union)
        self._emit_doc(union.doc)
        derive_traits = list(DERIVE_TRAITS)
        if self._can_derive_eq(union):
            derive_traits.append('Eq')
        self.emit(f'#[derive({", ".join(derive_traits)})]')
        if not union.closed:
            self.emit('#[non_exhaustive] // variants may be added in the future')
        with self.block(f'pub enum {enum_name}'):
            for field in union.all_fields:
                if field.catch_all:
                    # Handle the 'Other' variant at the end.
                    continue
                self._emit_doc(field.doc)
                variant_name = self.enum_variant_name(field)
                if isinstance(field.data_type, ir.Void):
                    self.emit(f'{variant_name},')
                else:
                    self.emit(f'{variant_name}({self._rust_type(field.data_type)}),')
            if not union.closed:
                self._emit_other_variant()
        self.emit()

        self._impl_serde_for_union(union)

        if self._is_error_type(union):
            self._impl_error(union)
        elif self.namespace_name_raw(self._current_namespace) + "::" + self._rust_type(union) in EXTRA_DISPLAY_TYPES:
            self._impl_display(union)
        elif union.name == "RateLimitReason":
            print(self._rust_type(union))

        if union.parent_type:
            self._impl_from_for_union(union, union.parent_type)

    def _emit_route(self, ns: str, fn: ir.ApiRoute, auth_trait: Optional[str] = None, as_async: bool = False) -> None:
        # work around lazy init messing with mypy
        assert fn.attrs is not None
        assert fn.arg_data_type is not None
        assert fn.result_data_type is not None
        assert fn.error_data_type is not None

        route_name = self.route_name(fn)
        host = fn.attrs.get('host', 'api')
        if host == 'api':
            endpoint = 'crate::client_trait_common::Endpoint::Api'
        elif host == 'content':
            endpoint = 'crate::client_trait_common::Endpoint::Content'
        elif host == 'notify':
            endpoint = 'crate::client_trait_common::Endpoint::Notify'
        else:
            raise RuntimeError(f'ERROR: unsupported endpoint: {host}')

        mod = 'async_client_trait' if as_async else 'client_trait'
        if auth_trait is None:
            auths_str = fn.attrs.get('auth', 'user')
            auths = list(map(lambda s: s.strip(), auths_str.split(',')))
            auths.sort()
            if auths == ['user']:
                auth_trait = f'crate::{mod}::UserAuthClient'
            elif auths == ['team']:
                auth_trait = f'crate::{mod}::TeamAuthClient'
            elif auths == ['app']:
                auth_trait = f'crate::{mod}::AppAuthClient'
            elif auths == ['app', 'user']:
                # This is kind of lame, but there's no way to have a marker trait for either User
                # OR App auth, so to get around this, we'll emit two functions, one for each.

                # Emit the User auth route with no suffix via a recursive call.
                self._emit_route(ns, fn, f'crate::{mod}::UserAuthClient', as_async=as_async)

                # Now modify the name to add a suffix, and emit the App auth version by continuing.
                route_name += "_app_auth"
                auth_trait = f'crate::{mod}::AppAuthClient'
            elif auths == ['noauth']:
                auth_trait = f'crate::{mod}::NoauthClient'
            else:
                raise Exception(f'route {ns}/{route_name}: unsupported auth type(s): {auths_str}')

        # This is the name of the HTTP route. Almost the same as the 'route_name', but without any
        # mangling to avoid Rust keywords and such.
        if fn.version > 1:
            name_with_version = f'{fn.name}_v{fn.version}'
        else:
            name_with_version = fn.name

        self._emit_doc(fn.doc)

        if fn.attrs.get('is_preview'):
            if fn.doc:
                self.emit('///')
            self.emit('/// # Stability')
            self.emit('/// *PREVIEW*: This function may change or disappear without notice.')
            self.emit('#[cfg(feature = "unstable")]')
            self.emit('#[cfg_attr(docsrs, doc(cfg(feature = "unstable")))]')

        if fn.deprecated:
            if fn.deprecated.by:
                self.emit(f'#[deprecated(note = "replaced by {self.route_name(fn.deprecated.by)}")]')
            else:
                self.emit('#[deprecated]')

        arg_void = isinstance(fn.arg_data_type, ir.Void)
        arg_type = self._rust_type(fn.arg_data_type)
        ret_type = self._rust_type(fn.result_data_type)
        style = fn.attrs.get('style', 'rpc')
        error_type = 'crate::NoError' if ir.is_void_type(fn.error_data_type) \
            else self._rust_type(fn.error_data_type)
        if style == 'rpc':
            with self.emit_rust_function_def(
                    route_name,
                    [f'client: &impl {auth_trait}']
                        + ([] if arg_void else [f'arg: &{arg_type}']),
                    f'Result<{ret_type}, crate::Error<{error_type}>>',
                    access='pub',
                    is_async=as_async):
                with self.conditional_wrapper(not as_async, 'crate::client_helpers::unwrap_async'):
                    self.emit_rust_fn_call(
                    'crate::client_helpers::request',
                    ['client',
                        endpoint,
                        'crate::client_trait_common::Style::Rpc',
                        f'"{ns}/{name_with_version}"',
                        '&()' if arg_void else 'arg',
                        'None'])
        elif style == 'download':
            with self.emit_rust_function_def(
                    route_name,
                    [f'client: &impl {auth_trait}']
                        + ([] if arg_void else [f'arg: &{arg_type}'])
                        + ['range_start: Option<u64>',
                            'range_end: Option<u64>'],
                    f'Result<crate::{mod}::HttpRequestResult<{ret_type}>, crate::Error<{error_type}>>',
                    access='pub',
                    is_async=as_async):
                with self.conditional_wrapper(not as_async, 'crate::client_helpers::unwrap_async_body'):
                    self.emit_rust_fn_call(
                        'crate::client_helpers::request_with_body',
                        ['client',
                            endpoint,
                            'crate::client_trait_common::Style::Download',
                            f'"{ns}/{name_with_version}"',
                            '&()' if arg_void else 'arg',
                            'None',
                            'range_start',
                            'range_end'],
                        end=None if as_async else ',')
                    if not as_async:
                        self.emit('client,')
        elif style == 'upload':
            with self.emit_rust_function_def(
                    route_name,
                    [f'client: &impl {auth_trait}']
                        + ([] if arg_void else [f'arg: &{arg_type}'])
                        + ['body: bytes::Bytes' if as_async else 'body: &[u8]'],
                    f'Result<{ret_type}, crate::Error<{error_type}>>',
                    access='pub',
                    is_async=as_async):
                with self.conditional_wrapper(not as_async, 'crate::client_helpers::unwrap_async'):
                    self.emit_rust_fn_call(
                        'crate::client_helpers::request',
                        ['client',
                            endpoint,
                            'crate::client_trait_common::Style::Upload',
                            f'"{ns}/{name_with_version}"',
                            '&()' if arg_void else 'arg',
                            'Some(crate::client_helpers::Body::from(body))'])
        else:
            raise RuntimeError(f'ERROR: unknown route style: {style}')
        self.emit()

    def _emit_alias(self, alias: ir.Alias) -> None:
        alias_name = self.alias_name(alias)
        assert isinstance(alias.data_type, ir.DataType)
        self.emit(f'pub type {alias_name} = {self._rust_type(alias.data_type)};')

    def _emit_other_variant(self) -> None:
        self.emit_wrapped_text(
                'Catch-all used for unrecognized values returned from the server.'
                ' Encountering this value typically indicates that this SDK version is'
                ' out of date.',
                prefix='/// ', width=100)
        self.emit('Other,')

    # Serialization

    def _impl_serde_for_struct(self, struct: ir.Struct) -> None:
        """
        Emit internal_deserialize() and possibly internal_deserialize_opt().
        internal_deserialize[_opt] takes a map and deserializes it into the struct. It reads the
        fields in whatever order; missing fields will be given their default value, or an error
        returned if they have no default. Errors will also be raised if a field is present more
        than once.
        The _opt deserializer returns a None if it reads exactly zero map keys, and is used for
        cases where the JSON has a tag, but omits all the fields to signify a null value. It is
        only emitted for types which have at least one required field, because if all fields are
        optional, there's no way to differentiate between a null value and one where all fields
        are default.
        """

        type_name = self.struct_name(struct)
        field_list_name = f'{fmt_shouting_snake(struct.name)}_FIELDS'
        self.generate_multiline_list(
            list(f'"{field.name}"' for field in struct.all_fields),
            before=f'const {field_list_name}: &[&str] = &',
            after=';',
            delim=('[', ']'))
        # Only emit the _opt deserializer if there are required fields.
        optional = len(struct.all_required_fields) > 0
        with self._impl_struct(struct):
            if optional:
                # Convenience wrapper around _opt for the more common case where the struct is
                # NOT optional.
                with self.emit_rust_function_def(
                        'internal_deserialize<\'de, V: ::serde::de::MapAccess<\'de>>',
                        ['map: V'],
                        f'Result<{type_name}, V::Error>',
                        access='pub(crate)'):
                    self.emit('Self::internal_deserialize_opt(map, false)'
                              '.map(Option::unwrap)')
                self.emit()
            else:
                self.emit('// no _opt deserializer')
            with self.emit_rust_function_def(
                    ('internal_deserialize_opt' if optional else 'internal_deserialize')
                    + '<\'de, V: ::serde::de::MapAccess<\'de>>',
                    ['mut map: V']
                    + (['optional: bool'] if optional else []),
                    f'Result<Option<{type_name}>, V::Error>' if optional
                            else f'Result<{type_name}, V::Error>',
                    access='pub(crate)'):
                if len(struct.all_fields) == 0:
                    self.emit('// ignore any fields found; none are presently recognized')
                    self.emit('crate::eat_json_fields(&mut map)?;')
                    if optional:
                        self.emit('Ok(None)')
                    else:
                        self.emit(f'Ok({type_name} {{}})')
                else:
                    for field in struct.all_fields:
                        self.emit(f'let mut field_{self.field_name(field)} = None;')
                    if optional:
                        self.emit('let mut nothing = true;')
                    with self.block('while let Some(key) = map.next_key::<&str>()?'):
                        if optional:
                            self.emit('nothing = false;')
                        with self.block('match key'):
                            for field in struct.all_fields:
                                field_name = self.field_name(field)
                                with self.block(f'"{field.name}" =>'):
                                    with self.block(f'if field_{field_name}.is_some()'):
                                        self.emit('return Err(::serde::de::Error::duplicate_field('
                                                  f'"{field.name}"));')
                                    self.emit(f'field_{field_name} = Some(map.next_value()?);')
                            with self.block('_ =>'):
                                self.emit('// unknown field allowed and ignored')
                                self.emit('map.next_value::<::serde_json::Value>()?;')
                    if optional:
                        with self.block('if optional && nothing'):
                            self.emit('return Ok(None);')
                    with self.block(f'let result = {type_name}', delim=('{', '};')):
                        for field in struct.all_fields:
                            field_name = self.field_name(field)
                            if isinstance(field.data_type, ir.Nullable):
                                # None -> field is not present
                                # Some(None) -> field is present with null value
                                # Some(Some(x)) -> field is present and non-null
                                # First two are equivalent here, hence Option::flatten().
                                self.emit(f'{field_name}: field_{field_name}.and_then(Option::flatten),')
                            elif field.has_default:
                                default_value = self._default_value(field)
                                if isinstance(field.data_type, ir.String) \
                                        and not field.default:
                                    self.emit(f'{field_name}: field_{field_name}.unwrap_or_default(),')
                                elif (ir.is_primitive_type(ir.unwrap_aliases(field.data_type)[0])
                                        # Also, as a rough but effective heuristic, consider values
                                        # that have no parentheses in them to be "trivial", and
                                        # don't enclose them in a closure. This avoids running
                                        # afoul of the clippy::unnecessary_lazy_evaluations lint.
                                        or not "(" in default_value):
                                    self.emit(f'{field_name}: field_{field_name}.unwrap_or({default_value}),')
                                else:
                                    self.emit(f'{field_name}: field_{field_name}.unwrap_or_else(|| {default_value}),')
                            else:
                                self.emit(f'{field_name}: field_{field_name}.ok_or_else(|| '
                                          f'::serde::de::Error::missing_field("{field.name}"))?,')
                    if optional:
                        self.emit('Ok(Some(result))')
                    else:
                        self.emit('Ok(result)')
            if struct.all_fields:
                self.emit()
                with self.emit_rust_function_def(
                        'internal_serialize<S: ::serde::ser::Serializer>',
                        ['&self', 's: &mut S::SerializeStruct'],
                        'Result<(), S::Error>',
                        access='pub(crate)'):
                    self.emit('use serde::ser::SerializeStruct;')
                    for field in struct.all_fields:
                        if ir.is_nullable_type(field.data_type):
                            # note: Stone requires a field can't be nullable and also have a
                            # non-null default
                            with self.block(f'if let Some(val) = &self.{self.field_name(field)}'):
                                self.emit(f's.serialize_field("{field.name}", val)?;')
                        else:
                            fieldval = f'self.{self.field_name(field)}'
                            ctx: contextlib.AbstractContextManager
                            if field.has_default:
                                if isinstance(field.data_type, ir.String) and not field.default:
                                    ctx = self.block(f'if !{fieldval}.is_empty()')
                                elif isinstance(field.data_type, ir.Boolean):
                                    if field.default:
                                        ctx = self.block(f'if !{fieldval}')
                                    else:
                                        ctx = self.block(f'if {fieldval}')
                                else:
                                    ctx = self.block(f'if {fieldval} != ' + str(self._default_value(field)))
                            else:
                                ctx = contextlib.nullcontext()
                            with ctx:
                                self.emit(f's.serialize_field("{field.name}", &{fieldval})?;')
                    self.emit('Ok(())')
        self.emit()
        with self._impl_deserialize(self.struct_name(struct)):
            self.emit('// struct deserializer')
            self.emit('use serde::de::{MapAccess, Visitor};')
            self.emit('struct StructVisitor;')
            with self.block('impl<\'de> Visitor<\'de> for StructVisitor'):
                self.emit(f'type Value = {type_name};')
                with self.emit_rust_function_def(
                        'expecting',
                        ['&self', 'f: &mut ::std::fmt::Formatter<\'_>'],
                        '::std::fmt::Result'):
                    self.emit(f'f.write_str("a {struct.name} struct")')
                with self.emit_rust_function_def(
                        'visit_map<V: MapAccess<\'de>>',
                        ['self', 'map: V'],
                        'Result<Self::Value, V::Error>'):
                    self.emit(f'{type_name}::internal_deserialize(map)')
            self.emit(f'deserializer.deserialize_struct("{struct.name}", {field_list_name}, StructVisitor)')
        self.emit()
        with self._impl_serialize(type_name):
            self.emit('// struct serializer')
            self.emit('use serde::ser::SerializeStruct;')
            if not struct.all_fields:
                self.emit(f'serializer.serialize_struct("{struct.name}", 0)?.end()')
            else:
                self.emit(f'let mut s = serializer.serialize_struct("{struct.name}", {len(struct.all_fields)})?;')
                self.emit('self.internal_serialize::<S>(&mut s)?;')
                self.emit('s.end()')
        self.emit()

    def _impl_serde_for_polymorphic_struct(self, struct: ir.Struct) -> None:
        type_name = self.enum_name(struct)
        with self._impl_deserialize(type_name):
            self.emit('// polymorphic struct deserializer')
            self.emit('use serde::de::{self, MapAccess, Visitor};')
            self.emit('struct EnumVisitor;')
            with self.block('impl<\'de> Visitor<\'de> for EnumVisitor'):
                self.emit(f'type Value = {type_name};')
                with self.emit_rust_function_def(
                        'expecting',
                        ['&self', 'f: &mut ::std::fmt::Formatter<\'_>'],
                        '::std::fmt::Result'):
                    self.emit(f'f.write_str("a {struct.name} structure")')
                with self.emit_rust_function_def(
                        'visit_map<V: MapAccess<\'de>>',
                        ['self', 'mut map: V'],
                        'Result<Self::Value, V::Error>'):
                    with self.block('let tag = match map.next_key()?', after=';'):
                        self.emit('Some(".tag") => map.next_value()?,')
                        self.emit('_ => return Err(de::Error::missing_field(".tag"))')
                    with self.block('match tag'):
                        for subtype in struct.get_enumerated_subtypes():
                            variant_name = self.enum_variant_name(subtype)
                            if isinstance(subtype.data_type, ir.Void):
                                self.emit(f'"{subtype.name}" => Ok({type_name}::{variant_name}),')
                            elif isinstance(ir.unwrap_aliases(subtype.data_type)[0], ir.Struct) \
                                    and not subtype.data_type.has_enumerated_subtypes():
                                self.emit(f'"{subtype.name}" => Ok({type_name}::{variant_name}('
                                          f'{self._rust_type(subtype.data_type)}::internal_deserialize(map)?)),')
                            else:
                                with self.block(f'"{subtype.name}" =>'):
                                    with self.block(f'if map.next_key()? != Some("{subtype.name}")'):
                                        self.emit(f'Err(de::Error::missing_field("{subtype.name}"));')
                                    self.emit(f'Ok({type_name}::{variant_name}(map.next_value()?))')
                        if struct.is_catch_all():
                            with self.block('_ =>'):
                                # TODO(wfraser): it'd be cool to grab any fields in the parent,
                                # which are common to all variants, and stick them in the
                                # 'Other' enum vaiant.
                                # For now, just consume them and return a nullary variant.
                                self.emit('crate::eat_json_fields(&mut map)?;')
                                self.emit(f'Ok({type_name}::Other)')
                        else:
                            self.emit('_ => Err(de::Error::unknown_variant(tag, VARIANTS))')
            self.generate_multiline_list(
                list(f'"{field.name}"' for field in struct.get_enumerated_subtypes()),
                before='const VARIANTS: &[&str] = &',
                after=';',
                delim=('[', ']'))
            self.emit(f'deserializer.deserialize_struct("{struct.name}", VARIANTS, EnumVisitor)')
        self.emit()
        with self._impl_serialize(type_name):
            self.emit('// polymorphic struct serializer')
            self.emit('use serde::ser::SerializeStruct;')
            with self.block('match *self'):
                for subtype in struct.get_enumerated_subtypes():
                    variant_name = self.enum_variant_name(subtype)
                    with self.block(f'{type_name}::{variant_name}(ref x) =>'):
                        self.emit('let mut s = serializer.serialize_struct('
                                  f'"{type_name}", {len(subtype.data_type.all_fields) + 1})?;')
                        self.emit(f's.serialize_field(".tag", "{subtype.name}")?;')
                        self.emit('x.internal_serialize::<S>(&mut s)?;')
                        self.emit('s.end()')
                if struct.is_catch_all():
                    self.emit(f'{type_name}::Other => Err(::serde::ser::Error::custom('
                              '"cannot serialize unknown variant"))')
        self.emit()

    def _impl_serde_for_union(self, union: ir.Union) -> None:
        type_name = self.enum_name(union)
        with self._impl_deserialize(type_name):
            self.emit('// union deserializer')
            self.emit('use serde::de::{self, MapAccess, Visitor};')
            self.emit('struct EnumVisitor;')
            with self.block('impl<\'de> Visitor<\'de> for EnumVisitor'):
                self.emit(f'type Value = {type_name};')
                with self.emit_rust_function_def(
                        'expecting',
                        ['&self', 'f: &mut ::std::fmt::Formatter<\'_>'],
                        '::std::fmt::Result'):
                    self.emit(f'f.write_str("a {union.name} structure")')
                with self.emit_rust_function_def(
                        'visit_map<V: MapAccess<\'de>>',
                        ['self', 'mut map: V'],
                        'Result<Self::Value, V::Error>'):
                    with self.block('let tag: &str = match map.next_key()?', after=';'):
                        self.emit('Some(".tag") => map.next_value()?,')
                        self.emit('_ => return Err(de::Error::missing_field(".tag"))')
                    if len(union.all_fields) == 1 and union.all_fields[0].catch_all:
                        self.emit('// open enum with no defined variants')
                        self.emit('let _ = tag;') # hax
                        self.emit('crate::eat_json_fields(&mut map)?;')
                        self.emit(f'Ok({type_name}::Other)')
                    else:
                        with self.block('let value = match tag', after=';'):
                            for field in union.all_fields:
                                if field.catch_all:
                                    # Handle the 'Other' variant at the end.
                                    continue
                                variant_name = self.enum_variant_name(field)
                                ultimate_type = ir.unwrap(field.data_type)[0]
                                if isinstance(field.data_type, ir.Void):
                                    self.emit(f'"{field.name}" => {type_name}::{variant_name},')
                                elif isinstance(ultimate_type, ir.Struct) \
                                        and not ultimate_type.has_enumerated_subtypes():
                                    if isinstance(ir.unwrap_aliases(field.data_type)[0], ir.Nullable):
                                        # A nullable here means we might have more fields that can be
                                        # deserialized into the inner type, or we might have nothing,
                                        # meaning None.
                                        if not ultimate_type.all_required_fields:
                                            raise RuntimeError(f'{union.name}.{field.name}:'
                                                               ' an optional struct with no required fields is'
                                                               ' ambiguous')
                                        self.emit(f'"{field.name}" => {type_name}::{variant_name}('
                                                  f'{self._rust_type(ultimate_type)}::internal_deserialize_opt('
                                                  '&mut map, true)?),')
                                    else:
                                        self.emit(f'"{field.name}" => {type_name}::{variant_name}('
                                                  f'{self._rust_type(field.data_type)}::internal_deserialize('
                                                  '&mut map)?),')
                                else:
                                    with self.block(f'"{field.name}" =>'):
                                        with self.block('match map.next_key()?'):
                                            self.emit(f'Some("{field.name}") => {type_name}::{variant_name}('
                                                      'map.next_value()?),')
                                            if isinstance(ir.unwrap_aliases(field.data_type)[0],
                                                          ir.Nullable):
                                                # if it's null, the field can be omitted entirely
                                                self.emit(f'None => {type_name}::{variant_name}(None),')
                                            else:
                                                self.emit('None => return Err('
                                                          f'de::Error::missing_field("{field.name}")),')
                                            self.emit('_ => return Err(de::Error::unknown_field('
                                                      'tag, VARIANTS))')
                            if not union.closed:
                                self.emit(f'_ => {type_name}::Other,')
                            else:
                                self.emit('_ => return Err(de::Error::unknown_variant(tag, VARIANTS))')
                        self.emit('crate::eat_json_fields(&mut map)?;')
                        self.emit('Ok(value)')
            self.generate_multiline_list(
                    list(f'"{field.name}"' for field in union.all_fields),
                    before='const VARIANTS: &[&str] = &',
                    after=';',
                    delim=('[', ']'),)
            self.emit(f'deserializer.deserialize_struct("{union.name}", VARIANTS, EnumVisitor)')
        self.emit()
        with self._impl_serialize(type_name):
            self.emit('// union serializer')
            if len(union.all_fields) == 1 and union.all_fields[0].catch_all:
                # special case: an open union with no variants defined.
                self.emit('#![allow(unused_variables)]')
                self.emit('Err(::serde::ser::Error::custom("cannot serialize an open union with '
                          'no defined variants"))')
            else:
                self.emit('use serde::ser::SerializeStruct;')
                with self.block('match *self'):
                    for field in union.all_fields:
                        if field.catch_all:
                            # Handle the 'Other' variant at the end.
                            continue
                        variant_name = self.enum_variant_name(field)
                        if isinstance(field.data_type, ir.Void):
                            with self.block(f'{type_name}::{variant_name} =>'):
                                self.emit('// unit')
                                self.emit(f'let mut s = serializer.serialize_struct("{union.name}", 1)?;')
                                self.emit(f's.serialize_field(".tag", "{field.name}")?;')
                                self.emit('s.end()')
                        else:
                            ultimate_type = ir.unwrap(field.data_type)[0]
                            needs_x = not (isinstance(field.data_type, ir.Struct)
                                           and not field.data_type.all_fields)
                            ref_x = 'ref x' if needs_x else '_'
                            with self.block(f'{type_name}::{variant_name}({ref_x}) =>'):
                                if self.is_enum_type(ultimate_type):
                                    # Inner type is a union or polymorphic struct; need to always
                                    # emit another nesting level.
                                    self.emit('// union or polymporphic struct')
                                    self.emit(f'let mut s = serializer.serialize_struct("{union.name}", 2)?;')
                                    self.emit(f's.serialize_field(".tag", "{field.name}")?;')
                                    self.emit(f's.serialize_field("{field.name}", x)?;')
                                    self.emit('s.end()')
                                elif isinstance(ir.unwrap_aliases(field.data_type)[0], ir.Nullable):
                                    self.emit('// nullable (struct or primitive)')
                                    # If it's nullable and the value is None, just emit the tag and
                                    # nothing else, otherwise emit the fields directly at the same
                                    # level.
                                    num_fields = 1 if ir.is_primitive_type(ultimate_type) \
                                        else len(ultimate_type.all_fields) + 1
                                    self.emit(f'let n = if x.is_some() {{ {num_fields + 1} }} else {{ 1 }};')
                                    self.emit(f'let mut s = serializer.serialize_struct("{union.name}", n)?;')
                                    self.emit(f's.serialize_field(".tag", "{field.name}")?;')
                                    with self.block('if let Some(ref x) = x'):
                                        if ir.is_primitive_type(ultimate_type):
                                            self.emit(f's.serialize_field("{field.name}", &x)?;')
                                        else:
                                            self.emit('x.internal_serialize::<S>(&mut s)?;')
                                    self.emit('s.end()')
                                elif isinstance(ultimate_type, ir.Struct):
                                    self.emit('// struct')
                                    self.emit('let mut s = serializer.serialize_struct('
                                              f'"{union.name}", {len(ultimate_type.all_fields) + 1})?;')
                                    self.emit(f's.serialize_field(".tag", "{field.name}")?;')
                                    if ultimate_type.all_fields:
                                        self.emit('x.internal_serialize::<S>(&mut s)?;')
                                    self.emit('s.end()')
                                else:
                                    self.emit('// primitive')
                                    self.emit(f'let mut s = serializer.serialize_struct("{union.name}", 2)?;')
                                    self.emit(f's.serialize_field(".tag", "{field.name}")?;')
                                    self.emit(f's.serialize_field("{field.name}", x)?;')
                                    self.emit('s.end()')
                    if not union.closed:
                        self.emit(f'{type_name}::Other => Err(::serde::ser::Error::custom('
                                  '"cannot serialize \'Other\' variant"))')
        self.emit()

    # "extends" for structs means the subtype adds additional fields to the supertype, so we can
    # convert from the subtype to the supertype
    def _impl_from_for_struct(self, struct: ir.Struct, parent: ir.Struct) -> None:
        subtype = self._rust_type(struct)
        supertype = self._rust_type(parent)
        self.emit(f'// struct extends {supertype}')
        with self.block(f'impl From<{subtype}> for {supertype}'):
            if not parent.all_fields:
                with self.block(f'fn from(_: {subtype}) -> Self'):
                    return self.emit('Self {}')
            with self.block(f'fn from(subtype: {subtype}) -> Self'):
                with self.block('Self'):
                    for field in parent.all_fields:
                        field_name = self.field_name(field)
                        self.emit(f'{field_name}: subtype.{field_name},')

    # "extends" for polymorphic structs means it's one of the supertype's variants, so we can
    # convert from the subtype to the supertype.
    def _impl_from_for_polymorphic_struct(self, struct: ir.Struct, parent: ir.Struct) -> None:
        thistype = self._rust_type(struct)
        supertype = self._rust_type(parent)
        self.emit(f'// struct extends polymorphic struct {supertype}')
        with self.block(f'impl From<{thistype}> for {supertype}'):
            with self.block(f'fn from(subtype: {thistype}) -> Self'):
                for subtype in parent.get_enumerated_subtypes():
                    assert isinstance(subtype, ir.UnionField)
                    if subtype.data_type != struct:
                        continue
                    variant_name = self.enum_variant_name(subtype)
                    self.emit(f'{supertype}::{variant_name}(subtype)')

    # "extends" for unions means the subtype adds additional variants, so we can convert from the
    # supertype to the subtype.
    def _impl_from_for_union(self, union: ir.Union, parent: ir.Union) -> None:
        subtype = self._rust_type(union)
        supertype = self._rust_type(parent)
        self.emit(f'// union extends {supertype}')
        with self.block(f'impl From<{supertype}> for {subtype}'):
            with self.block(f'fn from(parent: {supertype}) -> Self'):
                with self.block(f'match parent'):
                    for field in parent.all_fields:
                        variant_name = self.enum_variant_name(field)
                        x = "" if isinstance(field.data_type, ir.Void) else "(x)"
                        self.emit(f'{supertype}::{variant_name}{x} => {subtype}::{variant_name}{x},')

    # Helpers

    def _emit_doc(self, doc_string: Optional[str], prefix: str = '///') -> None:
        if doc_string is not None:
            for idx, chunk in enumerate(doc_string.split('\n\n')):
                if idx != 0:
                    self.emit(prefix)
                docf = lambda tag, val: self._docf(tag, val)
                self.emit_wrapped_text(
                        self.process_doc(chunk, docf),
                        prefix=prefix + ' ', width=100)

    def _docf(self, tag: str, val: str) -> str:
        if tag == 'route':
            if ':' in val:
                val, vstr = val.split(':')
                version = int(vstr)
            else:
                version = 1
            if '.' in val:
                ns, route = val.split('.')
            else:
                route = val
                ns = self._current_namespace

            rust_fn = self.route_name_raw(route, version)
            if ns != self._current_namespace:
                label = ns + '::' + rust_fn
            else:
                label = rust_fn

            target = f'crate::{ns}::{rust_fn}'
            return f'[`{label}()`]({target})'
        elif tag == 'field':
            if '.' in val:
                cls_name, field = val.rsplit('.', 1)
                assert '.' not in cls_name  # dunno if this is even allowed, but we don't handle it
                typ = self._all_types[self._current_namespace][cls_name]
                type_name = self._rust_type(typ)
                if self.is_enum_type(typ):
                    if isinstance(typ, ir.Struct) and typ.has_enumerated_subtypes() \
                            and typ.fields and field in (field.name for field in typ.fields):
                        # This is actually a link to a field in a polymorphic struct, not a enum
                        # variant. Because Rust doesn't have polymorphism, we make the fields be
                        # present on all enum variants, so this is a link to a field in the current
                        # type. Rustdoc doesn't let you link to a field, just the type, but we're
                        # already at that page, so don't bother with emitting an actual link.
                        # Hopefully we're documenting one of the variants right now, or else this
                        # is going to look weird.
                        field = self.field_name_raw(field)
                        return f'`{field}`'
                    field = self.enum_variant_name_raw(field)
                    return f'[`{type_name}::{field}`]({type_name}::{field})'
                else:
                    field = self.field_name_raw(field)
                    # we can't link to the field itself, so just link to the struct
                    return f'[`{type_name}::{field}`]({type_name})'
            else:
                # link is relative to the current type
                type_name = self._rust_type(self._current_type)
                if self.is_enum_type(self._current_type):
                    variant_name = self.enum_variant_name_raw(val)
                    return f'[`{variant_name}`]({type_name}::{variant_name})'
                else:
                    field_name = self.field_name_raw(val)
                    # we could, but don't bother linking to the struct because we're already there.
                    # return f'[`{field_name}`]({current_rust_type})'
                    return f'`{field_name}`'
        elif tag == 'type':
            if '.' in val:
                ns, typ_name = val.split('.')
                typ = self._all_types[ns][typ_name]
                rust_name = self._rust_type(typ, no_qualify=True)
                full_rust_name = self._rust_type(typ)
                return f'[`{ns}::{rust_name}`]({full_rust_name})'
            else:
                typ = self._all_types[self._current_namespace][val]
                rust_name = self._rust_type(typ)
                return f'[`{rust_name}`]({rust_name})'
        elif tag == 'link':
            title, url = val.rsplit(' ', 1)
            return f'[{title}]({url})'
        elif tag == 'val':
            if val == 'null':
                return '`None`'
            else:
                return f'`{val}`'
        else:
            print(f"WARNING: unrecognized link tag '{tag}'")
            return f'`{val}`'

    @contextmanager
    def _impl_deserialize(self, type_name: str) -> Iterator[None]:
        with self.block(f'impl<\'de> ::serde::de::Deserialize<\'de> for {type_name}'), \
                self.emit_rust_function_def(
                    'deserialize<D: ::serde::de::Deserializer<\'de>>',
                    ['deserializer: D'],
                    'Result<Self, D::Error>'):
            yield

    @contextmanager
    def _impl_serialize(self, type_name: str) -> Iterator[None]:
        with self.block(f'impl ::serde::ser::Serialize for {type_name}'), \
                self.emit_rust_function_def(
                    'serialize<S: ::serde::ser::Serializer>',
                    ['&self', 'serializer: S'],
                    'Result<S::Ok, S::Error>'):
            yield

    def _impl_default_for_struct(self, struct: ir.Struct) -> None:
        struct_name = self.struct_name(struct)
        with self.block(f'impl Default for {struct_name}'):
            with self.emit_rust_function_def('default', [], 'Self'):
                with self.block(struct_name):
                    for field in struct.all_fields:
                        name = self.field_name(field)
                        value = self._default_value(field)
                        self.emit(f'{name}: {value},')

    @contextmanager
    def _impl_struct(self, struct: ir.Struct) -> Iterator[None]:
        with self.block(f'impl {self.struct_name(struct)}'):
            yield

    def _emit_new_for_struct(self, struct: ir.Struct) -> None:
        struct_name = self.struct_name(struct)
        first = True

        if struct.all_required_fields:
            with self.emit_rust_function_def(
                    'new',
                    [f'{self.field_name(field)}: {self._rust_type(field.data_type)}'
                        for field in struct.all_required_fields],
                    'Self',
                    access='pub'):
                with self.block(struct_name):
                    for field in struct.all_required_fields:
                        # shorthand assignment
                        self.emit(f'{self.field_name(field)},')
                    for field in struct.all_optional_fields:
                        name = self.field_name(field)
                        value = self._default_value(field)
                        self.emit(f'{name}: {value},')
            first = False

        for field in struct.all_optional_fields:
            if first:
                first = False
            else:
                self.emit()

            field_name = self.field_name(field)
            if isinstance(field.data_type, ir.Nullable):
                # If it's a nullable type, the default is always None. Change the argument type to
                # the inner type, because if the user is using builder methods it means they don't
                # want the default, so making them type 'Some(...)' is redundant.
                field_type = field.data_type.data_type
                value = 'Some(value)'
            else:
                field_type = field.data_type
                value = 'value'

            with self.emit_rust_function_def(
                    f'with_{field_name}',
                    ['mut self', f'value: {self._rust_type(field_type)}'],
                    'Self',
                    access='pub'):
                self.emit(f'self.{field_name} = {value};')
                self.emit('self')

    def _default_value(self, field: ir.StructField) -> str:
        if isinstance(field.data_type, ir.Nullable):
            return 'None'
        elif ir.is_numeric_type(ir.unwrap_aliases(field.data_type)[0]):
            return str(field.default)
        elif isinstance(field.default, ir.TagRef):
            default_variant = None
            for variant in field.default.union_data_type.all_fields:
                if variant.name == field.default.tag_name:
                    default_variant = variant
            if default_variant is None:
                raise RuntimeError(f'ERROR: didn\'t find matching variant of {field.data_type.name}:'
                                   f' {field.default.tag_name}')
            typ = self._rust_type(field.default.union_data_type)
            variant = self.enum_variant_name(default_variant)
            return f'{typ}::{variant}'
        elif isinstance(field.data_type, ir.Boolean):
            if field.default:
                return 'true'
            else:
                return 'false'
        elif isinstance(field.data_type, ir.String):
            if not field.default:
                return 'String::new()'
            else:
                return f'"{field.default}".to_owned()'
        else:
            print(f'WARNING: unhandled default value {field.default}')
            print(f'    in field: {field}')
            if isinstance(field.data_type, ir.Alias):
                print('    unwrapped alias:', ir.unwrap_aliases(field.data_type)[0])
            return str(field.default)

    def _can_derive_eq(self, typ: ir.DataType) -> bool:
        if isinstance(typ, ir.Float32) or isinstance(typ, ir.Float64):
            # These are the only primitive types that don't have strict equality.
            return False

        # Check for various kinds of compound types and check all fields:
        if hasattr(typ, "data_type"):
            return self._can_derive_eq(typ.data_type)
        if isinstance(typ, ir.Struct) and typ.has_enumerated_subtypes():
            for styp in typ.get_enumerated_subtypes():
                if not self._can_derive_eq(styp):
                    return False
            return True
        if hasattr(typ, "all_fields"):
            for field in typ.all_fields:
                if not self._can_derive_eq(field.data_type):
                    return False
            return True

        # All other primitive types are strict-comparable.
        return True

    def _needs_explicit_default(self, field: ir.StructField) -> bool:
        if isinstance(field.data_type, ir.Nullable):
            # default is always None
            return False
        elif not field.has_default or isinstance(field.default, ir.TagRef):
            return True
        elif ir.is_numeric_type(ir.unwrap_aliases(field.data_type)[0]):
            return field.default != 0
        elif isinstance(field.data_type, ir.Boolean):
            return bool(field.default)
        elif isinstance(field.data_type, ir.String):
            return len(field.default) != 0
        else:
            print(f'WARNING: don\'t know if field {field} can have derived Default trait')
            print('its data type is', field.data_type)
            print('its default is', field.default)
            return True

    def _is_error_type(self, typ: ir.DataType) -> bool:
        return typ in self._error_types

    def _impl_error(self, typ: ir.DataType) -> None:
        type_name = self.enum_name(typ)

        # N.B.: error types SHOULD always be enums, but there's at least one type used as the error
        # return type of a route that's actually a struct, so this function needs to be able to
        # handle those as well. Passing a struct to get_enum_variants() will result in an empty
        # list, so this will just fall through to the end where we spit out a Debug repr for
        # Display, which is fine.
        variants = self.get_enum_variants(typ)

        with self.block(f'impl ::std::error::Error for {type_name}'):
            has_inner = list(v for v in variants if self._is_error_type(v.data_type))
            if has_inner:
                with self.emit_rust_function_def(
                        'source', ['&self'], 'Option<&(dyn ::std::error::Error + \'static)>'):
                    with self.block('match self'):
                        for variant in has_inner:
                            variant_name = self.enum_variant_name(variant)
                            self.emit(f'{type_name}::{variant_name}(inner) => Some(inner),')
                        if not self.is_closed_union(typ) or has_inner != variants:
                            self.emit('_ => None,')

        self.emit()
        self._impl_display(typ)

    def _impl_display(self, typ: ir.DataType) -> None:
        type_name = self.enum_name(typ)
        variants = self.get_enum_variants(typ)

        with self.block(f'impl ::std::fmt::Display for {type_name}'):
            with self.emit_rust_function_def(
                    'fmt',
                    ['&self', 'f: &mut ::std::fmt::Formatter<\'_>'],
                    '::std::fmt::Result'):

                # Find variants that have documentation and/or an inner value, and use that for the
                # Display representation of the error.
                doc_variants = []
                any_skipped = False
                for variant in variants:
                    variant_name = self.enum_variant_name(variant)
                    var_exp = f'{type_name}::{variant_name}'
                    msg = ''
                    args = ''
                    if variant.doc:
                        # Use the first line of documentation.
                        msg = variant.doc.split('\n')[0]

                        # If the line has doc references, it's not going to make a good display
                        # string, so only include it if it has none:
                        if msg != self.process_doc(msg, lambda tag, value: ''):
                            msg = ""

                    inner_fmt = ''
                    if self._is_error_type(variant.data_type):
                        # include the Display representation of the inner error.
                        inner_fmt = '{}'
                    elif not ir.is_void_type(variant.data_type):
                        # Include the Debug representation of the inner value.
                        inner_fmt = '{:?}'

                        if not msg:
                            # But if there's no message here already, prefix it with the name of the
                            # variant so there's some context.
                            msg = variant.name

                    if inner_fmt:
                        # Special case: if the inner value is an Option, spit out two match cases,
                        # one for if it's Some, and one for None.
                        # This is to avoid printing something like "foobar: None" if we're using
                        # the Debug repr, which looks confusing and adds nothing of value to the
                        # message.
                        if ir.is_nullable_type(variant.data_type):
                            doc_variants.append(f'{var_exp}(None) => f.write_str("{msg}"),')
                            var_exp += '(Some(inner))'
                        else:
                            var_exp += '(inner)'

                        if msg.endswith('.'):
                            msg = msg[:-1]
                        if msg:
                            msg += ': '
                        msg += inner_fmt
                        args = 'inner'

                    if msg:
                        if not args:
                            doc_variants.append(f'{var_exp} => f.write_str("{msg}"),')
                        else:
                            doc_variants.append(f'{var_exp} => write!(f, "{msg}", {args}),')
                    else:
                        any_skipped = True
                # for variant in variants

                if doc_variants:
                    with self.block('match self'):
                        for match_case in doc_variants:
                            self.emit(match_case)

                        if not self.is_closed_union(typ) or any_skipped:
                            # fall back on the Debug representation
                            self.emit('_ => write!(f, "{:?}", *self),')
                else:
                    # skip the whole match block and just use the Debug representation
                    self.emit('write!(f, "{:?}", *self)')
        self.emit()

    # Naming Rules

    def _rust_type(self, typ: ir.DataType, no_qualify: bool = False) -> str:
        return self.rust_type(typ, self._current_namespace, no_qualify)
