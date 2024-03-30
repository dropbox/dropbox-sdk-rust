from __future__ import annotations

import datetime
import os.path
import re
import string
import sys
from _ast import Module
from contextlib import contextmanager
from typing import Any, Dict, Iterator, Optional, Protocol

try:
    import re._parser as sre_parse  # type: ignore
except ImportError:    # Python < 3.11
    import sre_parse


from rust import RustHelperBackend, REQUIRED_NAMESPACES
from stone import ir
from stone.backends.python_helpers import fmt_class as fmt_py_class


class Permissions(object):
    @property
    def permissions(self) -> list[str]:
        # For generating tests, make sure we include any internal
        # fields/structs if we're using internal specs. If we're not using
        # internal specs, this is a no-op, so just do it all the time.  Note
        # that this only needs to be done for json serialization, the struct
        # definitions will include all fields, all the time.
        return ['internal']


#JsonEncodeType = Callable[[Any, object, list[str], **kwargs], str]
class JsonEncodeType(Protocol):
    def __call__(
            self,
            data_type: Any,
            obj: object,
            permissions: Permissions,
            _1: Any = None,
            _2: bool = False,
            _3: bool = False,
    ) -> str:
        ...


class TestBackend(RustHelperBackend):
    def __init__(self, target_folder_path: str, args: list[str]) -> None:
        super(TestBackend, self).__init__(target_folder_path, args)

        # Don't import other generators until here, otherwise stone.cli will
        # call them with its own arguments, in addition to the TestBackend.
        from stone.backends.python_types import PythonTypesBackend
        self.target_path = target_folder_path
        self.ref_path = os.path.join(target_folder_path, 'reference')
        self.reference = PythonTypesBackend(self.ref_path, args + ["--package", "reference"])
        self.reference_impls: Dict[str, Module] = {}

    # Make test values for this type.
    # If it's a union or polymorphic type, make values for all variants.
    # If the type or any of its variants have optional fields, also make two versions: one with all
    # fields filled in, and one with all optional fields omitted. This helps catch backwards-compat
    # issues as well as checking (de)serialization of None.
    def make_test_values(self, typ: ir.Struct | ir.Union) -> list[TestStruct | TestUnion | TestPolymorphicStruct]:
        vals: list[TestStruct | TestUnion | TestPolymorphicStruct] = []
        if isinstance(typ, ir.Struct):
            if typ.has_enumerated_subtypes():
                for variant in typ.get_enumerated_subtypes():
                    vals.append(TestPolymorphicStruct(
                        self, typ, self.reference_impls, variant, no_optional_fields=False))
                    if ir.is_struct_type(variant.data_type) \
                            and variant.data_type.all_optional_fields:
                        vals.append(TestPolymorphicStruct(
                            self, typ, self.reference_impls, variant, no_optional_fields=True))
            else:
                vals.append(TestStruct(self, typ, self.reference_impls, no_optional_fields=False))
                if typ.all_optional_fields:
                    vals.append(TestStruct(self, typ, self.reference_impls, no_optional_fields=True))
        elif isinstance(typ, ir.Union):
            for variant in typ.all_fields:
                vals.append(TestUnion(
                    self, typ, self.reference_impls, variant, no_optional_fields=False))
        else:
            raise RuntimeError(f'ERROR: type {typ} is neither struct nor union')
        return vals

    def generate(self, api: ir.Api) -> None:
        print('Generating Python reference code')
        self.reference.generate(api)
        with self.output_to_relative_path('reference/__init__.py'):
            self.emit('# this is the Stone-generated reference Python SDK')

        print('Loading reference code:')
        sys.path.insert(0, self.target_path)
        sys.path.insert(1, "stone")
        from stone.backends.python_rsrc.stone_serializers import json_encode
        for ns_name in api.namespaces:
            print('\t' + ns_name)
            python_ns = ns_name
            if ns_name == 'async':
                # hack to work around 'async' being a Python3 keyword
                python_ns = 'async_'
            self.reference_impls[ns_name] = __import__('reference.'+python_ns).__dict__[python_ns]

        print('Generating test code')
        for ns in api.namespaces.values():
            ns_name = self.namespace_name(ns)
            with self.output_to_relative_path(ns_name + '.rs'):
                self._emit_header()
                for typ in ns.data_types:
                    self._emit_tests(ns, typ, json_encode)

                    if self.is_closed_union(typ):
                        assert isinstance(typ, ir.Struct | ir.Union)
                        self._emit_closed_union_test(ns, typ)

                for route in ns.routes:
                    self._emit_route_test(ns, route, json_encode)

        with self.output_to_relative_path('mod.rs'):
            self._emit_header()
            self.emit('#[path = "../noop_client.rs"]')
            self.emit('pub mod noop_client;')
            self.emit()
            for ns_name in api.namespaces:
                if ns_name not in REQUIRED_NAMESPACES:
                    self.emit(f'#[cfg(feature = "dbx_{ns_name}")]')
                self.emit(f'mod {self.namespace_name_raw(ns_name)};')
                self.emit()

    def _emit_header(self) -> None:
        self.emit('// DO NOT EDIT')
        self.emit('// This file was @generated by Stone')
        self.emit()
        self.emit('#![allow(bad_style)]')
        self.emit()
        self.emit('#![allow(')
        self.emit('    clippy::float_cmp,')
        self.emit('    clippy::unreadable_literal,')
        self.emit('    clippy::cognitive_complexity,')
        self.emit('    clippy::collapsible_match,')
        self.emit('    clippy::bool_assert_comparison,')
        self.emit('    clippy::explicit_auto_deref,')
        self.emit(')]')
        self.emit()

    def _emit_tests(
            self,
            ns: ir.ApiNamespace,
            typ: ir.UserDefined,
            json_encode: JsonEncodeType,
    ) -> None:
        ns_name = self.namespace_name(ns)
        type_name = self.struct_name(typ)

        # The general idea here is to instantiate each type using the reference
        # Python code, put some random data in the fields, serialize it to
        # JSON, emit the JSON into the Rust test, have Rust deserialize it, and
        # emit assertions that the fields match. Then have Rust re-serialize to
        # JSON and desereialize it again, then check the fields of the
        # newly-deserialized struct. This verifies Rust's serializer.

        for test_value in self.make_test_values(typ):
            pyname = fmt_py_class(typ.name)
            rsname = self.struct_name(typ)

            json = json_encode(
                self.reference_impls[ns.name].__dict__[pyname + '_validator'],
                test_value.value,
                Permissions())

            # "other" is a hardcoded, special-cased tag used by Stone for the
            # catch-all variant of open unions. Let's rewrite it to something
            # else, to test that the unknown variant logic actually works.
            # Unfortunately this requires mega-hax of rewriting the JSON text,
            # because the Python serializer won't let us give an arbitrary
            # variant name.
            json = json.replace(
                    '{".tag": "other"',
                    '{".tag": "dropbox-sdk-rust-bogus-test-variant"')

            with self._test_fn(type_name + test_value.test_suffix()):
                self.emit(f'let json = r#"{json}"#;')
                self.emit(f'let x = ::serde_json::from_str::<::dropbox_sdk::{ns_name}::{rsname}>(json).unwrap();')
                test_value.emit_asserts(self, 'x')
                self.emit('assert_eq!(x, x.clone());')

                if test_value.is_serializable():
                    # now serialize it back to JSON, deserialize it again, and
                    # test it again.
                    self.emit()
                    self.emit('let json2 = ::serde_json::to_string(&x).unwrap();')
                    de = f'::serde_json::from_str::<::dropbox_sdk::{ns_name}::{rsname}>(&json2).unwrap()'

                    if typ.all_fields:
                        self.emit(f'let x2 = {de};')
                        test_value.emit_asserts(self, 'x2')
                        self.emit('assert_eq!(x, x2);')
                    else:
                        self.emit(f'{de};')
                else:
                    # assert that serializing it returns an error
                    self.emit('assert!(::serde_json::to_string(&x).is_err());')
            self.emit()

    def _emit_closed_union_test(self, ns: ir.ApiNamespace, typ: ir.Struct | ir.Union) -> None:
        ns_name = self.namespace_name(ns)
        type_name = self.struct_name(typ)
        with self._test_fn("ClosedUnion_" + type_name):
            self.emit('// This test ensures that an exhaustive match compiles.')
            self.emit(f'let x: Option<::dropbox_sdk::{ns_name}::{self.enum_name(typ)}> = None;')
            self.emit('match x {')
            with self.indent():
                var_exps = []
                for variant in self.get_enum_variants(typ):
                    v_name = self.enum_variant_name(variant)
                    var_exp = f'::dropbox_sdk::{ns_name}::{type_name}::{v_name}'
                    if not ir.is_void_type(variant.data_type):
                        var_exp += '(_)'
                    var_exps += [var_exp]

                self.generate_multiline_list(
                    ['None'] + [f'Some({exp})' for exp in var_exps],
                    sep=' | ',
                    skip_last_sep=True,
                    delim=('', ''),
                    after=' => ()')
            self.emit('}')
        self.emit()

    def _emit_route_test(
            self,
            ns: ir.ApiNamespace,
            route: ir.ApiRoute,
            json_encode: JsonEncodeType,
            auth_type: Optional[str] = None,
    ) -> None:
        assert route.arg_data_type
        assert route.result_data_type
        assert route.error_data_type
        assert route.attrs

        arg_typ = self.rust_type(route.arg_data_type, '', crate='dropbox_sdk')
        if arg_typ == '()':
            json = "{}"
        else:
            assert isinstance(route.arg_data_type, ir.Union | ir.Struct)
            arg_value = self.make_test_values(route.arg_data_type)[0]
            pyname = fmt_py_class(route.arg_data_type.name)
            json = json_encode(
                self.reference_impls[route.arg_data_type.namespace.name].__dict__[pyname + '_validator'],
                arg_value.value,
                Permissions())

        style = route.attrs.get('style', 'rpc')
        ok_typ = self.rust_type(_typ_or_void(route.result_data_type), '', crate='dropbox_sdk')
        if style == 'download':
            ok_typ = f'dropbox_sdk::client_trait::HttpRequestResult<{ok_typ}>'
        err_typ = self.rust_type(_typ_or_void(route.error_data_type), '', crate='dropbox_sdk')
        if err_typ == '()':
            err_typ = 'dropbox_sdk::NoError'
        ns_path = 'dropbox_sdk::routes::' + self.namespace_name(ns)
        fn_name = self.route_name(route)

        if auth_type is None:
            auths_str = route.attrs.get('auth', 'user')
            auths = list(map(lambda s: s.strip(), auths_str.split(',')))
            auths.sort()
            # See _emit_route() in rust.stoneg.py which enumerates all supported kinds
            if auths == ['app', 'user']:
                # This is the only kind of multi-auth supported.
                # Do the same shenanigans as when emitting the route code
                self._emit_route_test(ns, route, json_encode, 'user')

                fn_name += '_app_auth'
                auth_type = 'app'
            else:
                auth_type = auths[0]

        if route.attrs.get('is_preview'):
            self.emit('#[cfg(feature = "unstable")]')

        if route.deprecated:
            self.emit('#[allow(deprecated)]')

        with self._test_fn(f'route_{fn_name}'):
            if arg_typ != '()':
                self.emit(f'let arg: {arg_typ} = serde_json::from_str(r#"{json}"#).unwrap();')
            self.emit(f'let ret: dropbox_sdk::Result<Result<{ok_typ}, {err_typ}>>')
            with self.indent():
                self.emit(f'= {ns_path}::{fn_name}(')
                with self.indent():
                    self.emit(f'&super::noop_client::{auth_type}::Client,')
                    if arg_typ == '()':
                        self.emit('/* no args */')
                    else:
                        self.emit('&arg,')
                    if style == 'upload':
                        self.emit('&[]')
                    elif style == 'download':
                        self.emit('None,')
                        self.emit('None,')
                self.emit(');')
            self.emit('assert!(matches!(ret, Err(dropbox_sdk::Error::HttpClient(..))));')
        self.emit()

    @contextmanager
    def _test_fn(self, name: str) -> Iterator[None]:
        self.emit('#[test]')
        with self.emit_rust_function_def('test_' + name):
            yield


def _typ_or_void(typ: ir.DataType) -> ir.DataType:
    if typ is None:
        return ir.Void()
    else:
        return typ


class TestField(object):
    def __init__(
            self,
            name: str,
            python_value: Any,
            test_value: TestStruct | TestUnion | TestPolymorphicStruct,
            stone_type: ir.DataType,
            option: bool,
    ) -> None:
        self.name = name
        self.value = python_value
        self.test_value = test_value
        self.typ = stone_type
        self.option = option

    def emit_assert(self, codegen: RustHelperBackend, expression_path: str) -> None:
        extra = ('.' + self.name) if self.name else ''
        if self.option:
            if self.value is None:
                codegen.emit(f'assert!({expression_path}{extra}.is_none());')
                return
            expression = f'(*{expression_path}{extra}.as_ref().unwrap())'
        else:
            expression = expression_path + extra

        if isinstance(self.test_value, TestValue):
            self.test_value.emit_asserts(codegen, expression)
        elif ir.is_string_type(self.typ):
            codegen.emit(f'assert_eq!({expression}.as_str(), r#"{self.value}"#);')
        elif ir.is_numeric_type(self.typ):
            codegen.emit(f'assert_eq!({expression}, {self.value});')
        elif ir.is_boolean_type(self.typ):
            codegen.emit(f'assert_eq!({expression}, {"true" if self.value else "false"});')
        elif ir.is_timestamp_type(self.typ):
            assert isinstance(self.typ, ir.Timestamp)
            codegen.emit(f'assert_eq!({expression}.as_str(), "{self.value.strftime(self.typ.format)}");')
        elif ir.is_bytes_type(self.typ):
            codegen.emit(f'assert_eq!(&{expression}, &[{",".join(str(x) for x in self.value)}]);')
        else:
            raise RuntimeError(f'Error: assetion unhandled for type {self.typ}'
                               f' of field {self.name} with value {self.value}')


class TestValue(object):
    def __init__(self, rust_generator: RustHelperBackend) -> None:
        self.rust_generator = rust_generator
        self.fields: list[TestField] = []
        self.value = None

    def emit_asserts(self, codegen: RustHelperBackend, expression_path: str) -> None:
        raise NotImplementedError('you\'re supposed to implement TestValue.emit_asserts')

    def is_serializable(self) -> bool:
        # Not all types can round-trip back from Rust to JSON.
        return True

    def test_suffix(self) -> str:
        return ""


class TestStruct(TestValue):
    def __init__(
            self,
            rust_generator: RustHelperBackend,
            stone_type: ir.Struct,
            reference_impls: Dict[str, Any],
            no_optional_fields: bool,
    ) -> None:
        super(TestStruct, self).__init__(rust_generator)

        if stone_type.has_enumerated_subtypes():
            stone_type = stone_type.get_enumerated_subtypes()[0].data_type

        self._stone_type = stone_type
        self._reference_impls = reference_impls
        self._no_optional_fields = no_optional_fields

        py_name = fmt_py_class(stone_type.name)
        try:
            self.value = reference_impls[stone_type.namespace.name].__dict__[py_name]()
        except Exception as e:
            raise RuntimeError(f'Error instantiating value for {stone_type.name}: {e}')

        for field in stone_type.all_fields:
            if no_optional_fields and (field.has_default or ir.is_nullable_type(field.data_type)):
                # Construct a TestField to hold the default value and emit assertions for it, but
                # don't set the field value on this struct (so it is omitted from the JSON).
                field_value = test_field_with_value(
                    field.name,
                    field.default if field.has_default else None,
                    field.data_type,
                    rust_generator,
                    reference_impls)
            else:
                field_value = make_test_field(
                        field.name, field.data_type, rust_generator, reference_impls, no_optional_fields)
                if field_value is None:
                    raise RuntimeError(f'Error: incomplete type generated: {stone_type.name}')
                try:
                    setattr(self.value, field.name, field_value.value)
                except Exception as e:
                    raise RuntimeError(f'Error generating value for {stone_type.name}.{field.name}: {e}')
            self.fields.append(field_value)

    def emit_asserts(self, codegen: RustHelperBackend, expression_path: str) -> None:
        for field in self.fields:
            field.emit_assert(codegen, expression_path)

    def test_suffix(self) -> str:
        if self._no_optional_fields:
            return "_OnlyRequiredFields"
        else:
            return ""


class TestUnion(TestValue):
    def __init__(
            self,
            rust_generator: RustHelperBackend,
            stone_type: ir.Struct | ir.Union,  # Struct because TestPolymorphicStruct also uses this
            reference_impls: Dict[str, Module],
            variant: ir.UnionField,
            no_optional_fields: bool,
    ) -> None:
        super(TestUnion, self).__init__(rust_generator)
        self._stone_type = stone_type
        self._reference_impls = reference_impls
        self._rust_name = rust_generator.enum_name(stone_type)
        self._rust_variant_name = rust_generator.enum_variant_name_raw(variant.name)
        self._rust_namespace_name = rust_generator.namespace_name(stone_type.namespace)
        self._variant = variant
        self._no_optional_fields = no_optional_fields

        # We can't serialize the catch-all variant.
        self._is_serializable = not variant.catch_all

        self._inner_value = make_test_field(
            None, self._variant.data_type, rust_generator, reference_impls, no_optional_fields)

        if self._inner_value is None:
            raise RuntimeError(f'Error generating union variant value for {stone_type.name}.{variant.name}')

        self.value = self.get_from_inner_value(variant.name, self._inner_value)

    def get_from_inner_value(self, variant_name: str, generated_field: TestField) -> Any:
        pyname = fmt_py_class(self._stone_type.name)
        try:
            return self._reference_impls[self._stone_type.namespace.name] \
                    .__dict__[pyname](variant_name, generated_field.value)
        except Exception as e:
            raise RuntimeError(f'Error generating value for {self._stone_type.name}.{variant_name}: {e}')

    def has_other_variants(self) -> bool:
        return len(self._stone_type.all_fields) > 1 \
            or (isinstance(self._stone_type, ir.Union) and not self._stone_type.closed)

    def emit_asserts(self, codegen: RustHelperBackend, expression_path: str) -> None:
        if expression_path[0] == '(' and expression_path[-1] == ')':
            expression_path = expression_path[1:-1]  # strip off superfluous parens

        with codegen.block(f'match {expression_path}'):
            path = f'::dropbox_sdk::{self._rust_namespace_name}::{self._rust_name}::{self._rust_variant_name}'
            if ir.is_void_type(self._variant.data_type):
                codegen.emit(f'{path} => (),')
            elif codegen.is_nullary_struct(self._variant.data_type):
                codegen.emit(f'{path}(..) => (), // nullary struct')
            else:
                with codegen.block(f'{path}(ref v) =>'):
                    self._inner_value.emit_assert(codegen, '(*v)')

            if self.has_other_variants():
                codegen.emit('_ => panic!("wrong variant")')

    def is_serializable(self) -> bool:
        return not self._variant.catch_all

    def test_suffix(self) -> str:
        suf = "_" + self._rust_variant_name
        if self._no_optional_fields:
            suf += "_OnlyRequiredFields"
        return suf


class TestPolymorphicStruct(TestUnion):
    def get_from_inner_value(self, variant_name: str, generated_field: TestField) -> Any:
        return generated_field.value

    def has_other_variants(self) -> bool:
        return len(self._stone_type.get_enumerated_subtypes()) > 1 \
                or self._stone_type.is_catch_all()


class TestList(TestValue):
    def __init__(
            self,
            rust_generator: RustHelperBackend,
            stone_type: ir.DataType,
            reference_impls: Dict[str, Module],
    ) -> None:
        super(TestList, self).__init__(rust_generator)
        self._stone_type = stone_type
        self._reference_impls = reference_impls

        self._inner_value = make_test_field(
            None, stone_type, rust_generator, reference_impls, no_optional_fields=False)
        if self._inner_value is None:
            raise RuntimeError(f'Error generating value for list of {stone_type.name}')

        self.value = self._inner_value.value

    def emit_asserts(self, codegen: RustHelperBackend, expression_path: str) -> None:
        self._inner_value.emit_assert(codegen, expression_path + '[0]')


class TestMap(TestValue):
    def __init__(
            self,
            rust_generator: RustHelperBackend,
            stone_type: ir.Map,
            reference_impls: Dict[str, Module],
    ) -> None:
        super(TestMap, self).__init__(rust_generator)
        self._stone_type = stone_type
        self._reference_impls = reference_impls
        self._key_value = make_test_field(None, stone_type.key_data_type, rust_generator,
                                          reference_impls)
        self._val_value = make_test_field(None, stone_type.value_data_type, rust_generator,
                                          reference_impls)
        self.value = {self._key_value.value: self._val_value.value}

    def emit_asserts(self, codegen: RustHelperBackend, expression_path: str) -> None:
        key_str = f'["{self._key_value.value}"]'
        self._val_value.emit_assert(codegen, expression_path + key_str)


# Make a TestField with a specific value.
def test_field_with_value(
        field_name: str,
        value: Any,
        stone_type: ir.DataType,
        rust_generator: RustHelperBackend,
        reference_impls: Dict[str, Module],
) -> TestField:
    typ, option = ir.unwrap_nullable(stone_type)
    inner = None
    if ir.is_tag_ref(value):
        assert isinstance(stone_type, ir.Union)
        assert isinstance(value, ir.TagRef)
        # TagRef means we need to instantiate the named variant of this union, so find the right
        # field (variant) of the union and change the value to a TestUnion of it
        variant = None
        for f in stone_type.all_fields:
            assert isinstance(f, ir.UnionField)
            if f.name == value.tag_name:
                variant = f
                break
        assert variant, f"no appropriate variant found for tag name {value.tag_name}"
        inner = TestUnion(rust_generator, typ, reference_impls, variant, no_optional_fields=True)
        value = inner.value
    return TestField(
        rust_generator.field_name_raw(field_name),
        value,
        inner,
        typ,
        option)


# Make a TestField with an arbitrary value that satisfies constraints. If no_optional_fields is True
# then optional or nullable fields will be left unset.
def make_test_field(
        field_name: Optional[str],
        stone_type: ir.DataType,
        rust_generator: RustHelperBackend,
        reference_impls: Dict[str, Module],
        no_optional_fields: bool = False,
) -> TestField:
    rust_name = rust_generator.field_name_raw(field_name) if field_name is not None else None
    typ, option = ir.unwrap_nullable(stone_type)

    inner = None
    value = None
    if ir.is_struct_type(typ):
        if typ.has_enumerated_subtypes():
            variant = typ.get_enumerated_subtypes()[0]
            inner = TestPolymorphicStruct(rust_generator, typ, reference_impls, variant, no_optional_fields)
        else:
            inner = TestStruct(rust_generator, typ, reference_impls, no_optional_fields)
        value = inner.value
    elif ir.is_union_type(typ):
        # Pick the first tag.
        # We could generate tests for them all, but it would lead to a huge explosion of tests, and
        # the types themselves are tested elsewhere.
        if len(typ.fields) == 0:
            # there must be a parent type; go for it
            variant = typ.all_fields[0]
        else:
            variant = typ.fields[0]
        inner = TestUnion(rust_generator, typ, reference_impls, variant, no_optional_fields)
        value = inner.value
    elif ir.is_list_type(typ):
        inner = TestList(rust_generator, typ.data_type, reference_impls)
        value = [inner.value]
    elif ir.is_map_type(typ):
        inner = TestMap(rust_generator, typ, reference_impls)
        value = inner.value
    elif ir.is_string_type(typ):
        if typ.pattern:
            value = Unregex(typ.pattern, typ.min_length).generate()
        elif typ.min_length:
            value = 'a' * typ.min_length
        else:
            value = 'something'
    elif ir.is_numeric_type(typ):
        value = typ.max_value or typ.maximum or 1e307
    elif ir.is_boolean_type(typ):
        value = True
    elif ir.is_timestamp_type(typ):
        value = datetime.datetime.utcfromtimestamp(2**33 - 1)
    elif ir.is_bytes_type(typ):
        value = bytes([0, 1, 2, 3, 4, 5])
    elif not ir.is_void_type(typ):
        raise RuntimeError(f'Error: unhandled field type of {field_name}: {typ}')
    return TestField(rust_name, value, inner, typ, option)


class Unregex(object):
    """
    Generate a minimal string that passes a regex and optionally is of a given
    minimum length.
    """
    def __init__(self, regex_string: str, min_len: Optional[int] = None) -> None:
        self._min_len = min_len
        self._group_refs = {}
        self._tokens = sre_parse.parse(regex_string)

    def generate(self) -> str:
        return self._generate(self._tokens)

    def _generate(self, tokens: Any) -> str:
        result = ''
        for (opcode, argument) in tokens:
            opcode = str(opcode).lower()
            if opcode == 'literal':
                result += chr(argument)
            elif opcode == 'at':
                pass  # start or end anchor; nothing to add
            elif opcode == 'in':
                if argument[0][0] == 'negate':
                    rejects = []
                    for opcode, reject in argument[1:]:
                        if opcode == 'literal':
                            rejects.append(chr(reject))
                        elif opcode == 'range':
                            for i in range(reject[0], reject[1]):
                                rejects.append(chr(i))
                    choices = list(set(string.printable)
                                   .difference(string.whitespace)
                                   .difference(rejects))
                    result += choices[0]
                else:
                    result += self._generate([argument[0]])
            elif opcode == 'any':
                result += '*'
            elif opcode == 'range':
                result += chr(argument[0])
            elif opcode == 'branch':
                result += self._generate(argument[1][0])
            elif opcode == 'subpattern':
                group_number, add_flags, del_flags, sub_tokens = argument
                sub_result = self._generate(sub_tokens)
                self._group_refs[group_number] = sub_result
                result += sub_result
            elif opcode == 'groupref':
                result += self._group_refs[argument]
            elif opcode == 'min_repeat' or opcode == 'max_repeat':
                min_repeat, max_repeat, sub_tokens = argument
                if self._min_len:
                    n = max(min_repeat, min(self._min_len, max_repeat))
                else:
                    n = min_repeat
                sub_result = self._generate(sub_tokens) if n != 0 else ''
                result += str(sub_result) * n
            elif opcode == 'category':
                if argument == sre_parse.CATEGORY_WORD:
                    result += 'A'
                else:
                    raise NotImplementedError(f'category {argument}')
            elif opcode == 'assert_not':
                # let's just hope for the best...
                pass
            elif opcode == 'assert' or opcode == 'negate':
                # note: 'negate' is handled in the 'in' opcode
                raise NotImplementedError(f'regex opcode {opcode} not implemented')
            else:
                raise NotImplementedError(f'unknown regex opcode: {opcode}')
        return result
