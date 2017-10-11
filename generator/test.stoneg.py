import datetime
import os.path
import re
import string
import sys

from rust import RustHelperBackend
from stone import ir


class TestBackend(RustHelperBackend):
    def __init__(self, target_folder_path, args):
        super(TestBackend, self).__init__(target_folder_path, args)

        # Don't import other generators until here, otherwise stone.cli will
        # call them with its own arguments, in addition to the TestBackend.
        from stone.backends.python_types import PythonTypesBackend
        self.ref_path = os.path.join(target_folder_path, 'reference')
        self.reference = PythonTypesBackend(self.ref_path, args)
        self.reference_impls = {}

    def generate(self, api):
        print(u'Generating Python reference code')
        self.reference.generate(api)
        with self.output_to_relative_path('reference/__init__.py'):
            self.emit(u'# this is the Stone-generated reference Python SDK')

        print(u'Loading reference code:')
        sys.path.append(self.ref_path)
        from stone_serializers import json_encode
        for ns in api.namespaces:
            print('\t' + ns)
            self.reference_impls[ns] = __import__(ns)

        print(u'Generating test code')
        for ns in api.namespaces.values():
            with self.output_to_relative_path(ns.name + '.rs'):
                self._emit_header()
                for typ in ns.data_types:
                    type_name = self.struct_name(typ)

                    # the general idea here is to instantiate each type using
                    # the reference Python code, put some random data in the
                    # fields, serialize it to JSON, emit the JSON into the Rust
                    # test, have Rust deserialize it, and emit assertions that
                    # the fields match. Then have Rust re-serialize to JSON and
                    # desereialize it again, then check the fields of the
                    # newly-deserialized struct. This verifies Rust's
                    # serializer.

                    is_serializable = True
                    test_value = None
                    if ir.is_struct_type(typ):
                        if typ.has_enumerated_subtypes():
                            # TODO: generate tests for all variants
                            # for now, just pick the first variant
                            variant = typ.get_enumerated_subtypes()[0]
                            test_value = TestPolymorphicStruct(
                                self, typ, self.reference_impls, variant)
                        else:
                            test_value = TestStruct(self, typ, self.reference_impls)
                    elif ir.is_union_type(typ):
                        # TODO: generate tests for all variants
                        # for now, just pick the first variant

                        # prefer choosing from this type and not the parent if we can
                        variants = [field for field in typ.fields if not field.catch_all]
                        if len(variants) == 0:
                            # fall back to parent type's variants
                            variants = [field for field in typ.all_fields if not field.catch_all]

                        if not variants:
                            # Rust code will refuse to serialize a type with no variants (or only
                            # the catch-all variant), so don't bother testing that
                            is_serializable = False
                            variant = typ.all_fields[0]  # this assumes there's at least one
                        else:
                            variant = variants[0]

                        test_value = TestUnion(self, typ, self.reference_impls, variant)
                    else:
                        raise RuntimeError(u'ERROR: type {} is neither struct nor union'
                                           .format(typ))

                    json = json_encode(
                        self.reference_impls[ns.name].__dict__[typ.name + '_validator'],
                        test_value.value)
                    with self._test_fn(type_name):
                        self.emit(u'let json = r#"{}"#;'.format(json))
                        self.emit(u'let x = ::serde_json::from_str::<::dropbox_sdk::{}::{}>(json).unwrap();'
                                  .format(ns.name,
                                          self.struct_name(typ)))
                        test_value.emit_asserts(self, 'x')

                        if is_serializable:
                            # now serialize it back to JSON, deserialize it again, and test
                            # it again.
                            self.emit()
                            self.emit(u'let json2 = ::serde_json::to_string(&x).unwrap();')
                            de = u'::serde_json::from_str::<::dropbox_sdk::{}::{}>(&json2).unwrap()' \
                                 .format(ns.name,
                                         self.struct_name(typ))

                            if typ.all_fields:
                                self.emit(u'let x2 = {};'.format(de))
                                test_value.emit_asserts(self, 'x2')
                            else:
                                self.emit(u'{};'.format(de))
                        else:
                            # assert that serializing it returns an error
                            self.emit(u'assert!(::serde_json::to_string(&x).is_err());')
                        self.emit()

                # for typ
            # .rs test file
        # for ns

        with self.output_to_relative_path('mod.rs'):
            self._emit_header()
            for ns in api.namespaces:
                self.emit(u'mod {};'.format(ns))

    def _emit_header(self):
        self.emit(u'// DO NOT EDIT')
        self.emit(u'// This file was generated by Stone')
        self.emit()
        self.emit(u'#![allow(unknown_lints, bad_style, float_cmp)]')
        self.emit()

    def _test_fn(self, name):
        self.emit(u'#[test]')
        return self.emit_rust_function_def(u'test_' + name)


class TestField(object):
    def __init__(self, name, python_value, test_value, stone_type, option):
        self.name = name
        self.value = python_value
        self.test_value = test_value
        self.typ = stone_type
        self.option = option

    def emit_assert(self, codegen, expression_path):
        extra = ('.' + self.name) if self.name else ''
        if self.option:
            expression = '(*' + expression_path + extra + '.as_ref().unwrap())'
        else:
            expression = expression_path + extra

        if isinstance(self.test_value, TestValue):
            self.test_value.emit_asserts(codegen, expression)
        elif ir.is_string_type(self.typ):
            codegen.emit(u'assert_eq!({}.as_str(), r#"{}"#);'.format(
                expression, self.value))
        elif ir.is_numeric_type(self.typ):
            codegen.emit(u'assert_eq!({}, {});'.format(
                expression, self.value))
        elif ir.is_boolean_type(self.typ):
            codegen.emit(u'assert_eq!({}, {});'.format(
                expression, 'true' if self.value else 'false'))
        elif ir.is_timestamp_type(self.typ):
            codegen.emit(u'assert_eq!({}.as_str(), "{}");'.format(
                expression, self.value.strftime(self.typ.format)))
        else:
            raise RuntimeError(u'Error: assetion unhandled for type {} of field {}'
                               .format(self.typ, self.name))


class TestValue(object):
    def __init__(self, rust_generator):
        self.rust_generator = rust_generator
        self.fields = []
        self.value = None

    def emit_asserts(self, codegen, expression_path):
        raise NotImplementedError('you\'re supposed to implement TestValue.emit_asserts')


class TestStruct(TestValue):
    def __init__(self, rust_generator, stone_type, reference_impls):
        super(TestStruct, self).__init__(rust_generator)

        if stone_type.has_enumerated_subtypes():
            stone_type = stone_type.get_enumerated_subtypes()[0].data_type

        self._stone_type = stone_type
        self._reference_impls = reference_impls

        try:
            self.value = reference_impls[stone_type.namespace.name].__dict__[stone_type.name]()
        except Exception as e:
            raise RuntimeError(u'Error instantiating value for {}: {}'.format(stone_type.name, e))

        for field in stone_type.all_fields:
            field_value = make_test_field(
                    field.name, field.data_type, rust_generator, reference_impls)
            if field_value is None:
                raise RuntimeError(u'Error: incomplete type generated: {}'.format(stone_type.name))
            self.fields.append(field_value)
            try:
                setattr(self.value, field.name, field_value.value)
            except Exception as e:
                raise RuntimeError(u'Error generating value for {}.{}: {}'
                                   .format(stone_type.name, field.name, e))

    def emit_asserts(self, codegen, expression_path):
        for field in self.fields:
            field.emit_assert(codegen, expression_path)


class TestUnion(TestValue):
    def __init__(self, rust_generator, stone_type, reference_impls, variant):
        super(TestUnion, self).__init__(rust_generator)
        self._stone_type = stone_type
        self._reference_impls = reference_impls
        self._rust_name = rust_generator.enum_name(stone_type)
        self._rust_variant_name = rust_generator.enum_variant_name_raw(variant.name)
        self._variant_type = variant.data_type

        self._inner_value = make_test_field(
            None, self._variant_type, rust_generator, reference_impls)

        if self._inner_value is None:
            raise RuntimeError(u'Error generating union variant value for {}.{}'
                               .format(stone_type.name, variant.name))

        self.value = self.get_from_inner_value(variant.name, self._inner_value)

    def get_from_inner_value(self, variant_name, generated_field):
        try:
            return self._reference_impls[self._stone_type.namespace.name] \
                    .__dict__[self._stone_type.name](variant_name, generated_field.value)
        except Exception as e:
            raise RuntimeError(u'Error generating value for {}.{}: {}'
                               .format(self._stone_type.name, variant_name, e))

    def is_open(self):
        return len(self._stone_type.all_fields) > 1

    def emit_asserts(self, codegen, expression_path):
        if expression_path[0] == '(' and expression_path[-1] == ')':
                expression_path = expression_path[1:-1]  # strip off superfluous parens

        with codegen.block(u'match {}'.format(expression_path)):
            if ir.is_void_type(self._variant_type):
                codegen.emit(u'::dropbox_sdk::{}::{}::{} => (),'.format(
                    self._stone_type.namespace.name,
                    self._rust_name,
                    self._rust_variant_name))
            else:
                with codegen.block(u'::dropbox_sdk::{}::{}::{}(ref v) =>'.format(
                        self._stone_type.namespace.name,
                        self._rust_name,
                        self._rust_variant_name)):
                    self._inner_value.emit_assert(codegen, '(*v)')

            if self.is_open():
                codegen.emit(u'_ => panic!("wrong variant")')


class TestPolymorphicStruct(TestUnion):
    def get_from_inner_value(self, variant_name, generated_field):
        return generated_field.value

    def is_open(self):
        return len(self._stone_type.get_enumerated_subtypes()) > 1


class TestList(TestValue):
    def __init__(self, rust_generator, stone_type, reference_impls):
        super(TestList, self).__init__(rust_generator)
        self._stone_type = stone_type
        self._reference_impls = reference_impls

        self._inner_value = make_test_field(None, stone_type, rust_generator, reference_impls)
        if self._inner_value is None:
            raise RuntimeError(u'Error generating value for list of {}'.format(stone_type.name))

        self.value = self._inner_value.value

    def emit_asserts(self, codegen, expression_path):
        self._inner_value.emit_assert(codegen, expression_path + '[0]')


def make_test_field(field_name, stone_type, rust_generator, reference_impls):
    rust_name = rust_generator.field_name_raw(field_name) if field_name is not None else None
    typ, option = ir.unwrap_nullable(stone_type)

    inner = None
    value = None
    if ir.is_struct_type(typ):
        if typ.has_enumerated_subtypes():
            variant = typ.get_enumerated_subtypes()[0]
            inner = TestPolymorphicStruct(rust_generator, typ, reference_impls, variant)
        else:
            inner = TestStruct(rust_generator, typ, reference_impls)
        value = inner.value
    elif ir.is_union_type(typ):
        # pick the first tag
        if len(typ.fields) == 0:
            # there must be a parent type; go for it
            variant = typ.all_fields[0]
        else:
            variant = typ.fields[0]
        inner = TestUnion(rust_generator, typ, reference_impls, variant)
        value = inner.value
    elif ir.is_list_type(typ):
        inner = TestList(rust_generator, typ.data_type, reference_impls)
        value = [inner.value]
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
    elif not ir.is_void_type(typ):
        raise RuntimeError(u'Error: unhandled field type of {}: {}'.format(field_name, typ))
    return TestField(rust_name, value, inner, typ, option)


class Unregex(object):
    """
    Generate a minimal string that passes a regex and optionally is of a given
    minimum length.
    """
    def __init__(self, regex_string, min_len=None):
        self._min_len = min_len
        self._group_refs = {}
        self._tokens = re.sre_parse.parse(regex_string)

    def generate(self):
        return self._generate(self._tokens)

    def _generate(self, tokens):
        result = ''
        for (opcode, argument) in tokens:
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
                number, sub_tokens = argument
                sub_result = self._generate(sub_tokens)
                self._group_refs[number] = sub_result
                result += sub_result
            elif opcode == 'groupref':
                result += self._group_refs[argument]
            elif opcode == 'min_repeat' or opcode == 'max_repeat':
                min_repeat, max_repeat, sub_tokens = argument
                if self._min_len:
                    n = min(self._min_len, max_repeat)
                else:
                    n = min_repeat
                sub_result = self._generate(sub_tokens) if n != 0 else ''
                result += sub_result * n
            elif opcode == 'category':
                if argument == 'category_digit':
                    result += '0'
                else:
                    raise NotImplementedError('category {}'.format(argument))
            elif opcode == 'assert' or opcode == 'assert_not' \
                    or opcode == 'negate':  # note: 'negate' is handled in the 'in' opcode
                raise NotImplementedError('regex opcode {} not implemented'.format(opcode))
            else:
                raise NotImplementedError('unknown regex opcode: {}'.format(opcode))
        return result
