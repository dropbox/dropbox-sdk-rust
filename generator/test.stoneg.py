from stone import data_type
from stone.generator import CodeGenerator
import stone.data_type

import datetime
import imp
import os.path
import re
import string
import sys

class TestGenerator(CodeGenerator):
    def __init__(self, target_folder_path, args):
        super(TestGenerator, self).__init__(target_folder_path, args)

        # Don't import other generators until here, otherwise stone.cli will call them with its own
        # arguments, in addition to the TestGenerator.
        from stone.target.python_types import PythonTypesGenerator
        self.ref_path = os.path.join(target_folder_path, 'reference')
        self.reference = PythonTypesGenerator(self.ref_path, args)
        self.reference_impls = {}

        rust_gen = imp.load_source('rust_gen', 'generator/rust.stoneg.py')
        self.rust = rust_gen.RustGenerator(self.ref_path, args)

    def generate(self, api):
        print(u'Generating Python reference code')
        self.reference.generate(api)

        with self.output_to_relative_path('reference/__init__.py'):
            self.emit(u'# this is the Stone-generated reference Python SDK')

        print(u'Loading reference code')
        sys.path.append(self.ref_path)
        from stone_serializers import (json_encode, json_decode)
        for ns in api.namespaces:
            print(ns)
            self.reference_impls[ns] = __import__(ns)

        print(u'Generating test code')
        for ns in api.namespaces.values():
            with self.output_to_relative_path(ns.name + '.rs'):
                self._emit_header()
                for typ in ns.data_types:
                    type_name = self.rust._struct_name(typ)

                    # TODO
                    # the general idea here is to instantiate each type using the reference Python
                    # code, put some random data in the fields, serialize it to JSON, emit the
                    # JSON into the Rust test, have Rust deserialize it, and emit assertions that
                    # the fields match.

                    # For testing the serializer, emit code that instantiates each type in Rust,
                    # fills the fields with random data, serializes to JSON, then calls out to the
                    # reference Python code to deserialize and assert the fields match, and check
                    # the result of calling Python.

                    # Alternatively, it might be sufficient to do the deserializer test, and just
                    # make sure that the data can round-trip through the serializer unchanged.

                    # walk the fields down to the primitive leaves
                    # for each leaf, figure out if it's a boolean, string, or number
                    #   and generate a value and a path expression
                    # assemble a python object from these
                    # serialize the python object to json
                    # emit the json to the test
                    # emit a deserialize expression to the test
                    # for each leaf path generated, emit an assertion
                    # emit a serialize expression followed by another deserialize
                    # call the leaf assertions again

                    test_value = None
                    if data_type.is_struct_type(typ):
                        test_value = TestStruct(self.rust, typ, self.reference_impls)
                    elif data_type.is_union_type(typ):
                        pass
                    else:
                        print(u'ERROR: type {} is neither struct nor union'.format(typ))

                    if test_value is None or test_value.value is None:
                        self.emit(u'#[ignore]')
                        with self._test_fn(type_name):
                            self.emit(u'// test not implemented')
                        self.emit()
                    else:
                        try:
                            json = json_encode(
                                self.reference_impls[ns.name].__dict__[typ.name + '_validator'],
                                test_value.value)
                            with self._test_fn(type_name):
                                self.emit(u'let json = r#"{}"#;'.format(json))
                                de = u'serde_json::from_str::<dropbox_sdk::{}::{}>(json).unwrap()'.format(
                                    ns.name,
                                    self.rust._struct_name(typ))
                                if typ.all_fields:
                                    self.emit(u'let x = {};'.format(de))
                                    for expr, value in test_value.leaves:
                                        self.emit(u'assert_eq!(x.{}, {});'.format(
                                            expr,
                                            value))
                                else:
                                    self.emit(u'{};'.format(de))
                            self.emit()
                        except Exception as e:
                            print(u'Error serializing {}.{}: {}'.format(
                                ns.name, typ.name, e))
                            self.emit(u'#[ignore]')
                            with self._test_fn(type_name):
                                self.emit(u'// error generating test: {}'.format(e))
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
        self.emit(u'#![allow(bad_style)]')
        self.emit()
        self.emit(u'extern crate dropbox_sdk;')
        self.emit(u'extern crate serde_json;')
        self.emit()

    def _test_fn(self, name):
        self.emit(u'#[test]')
        return self.block(u'fn test_{}()'.format(name))

class TestStruct:
    def __init__(self, rust_generator, stone_type, reference_impls):
        self._rust_generator = rust_generator
        self._stone_type = stone_type
        self._reference_impls = reference_impls
        try:
            self.value = reference_impls[stone_type.namespace.name].__dict__[stone_type.name]()
        except Exception as e:
            print(u'Error instantiating value for {}: {}'.format(
                stone_type.name, e))
            raise
        self.leaves = []
        for field in stone_type.all_fields:
            value = self._generate_field_value(field, field.data_type)
            if value is None:
                self.value = None # we have an incomplete type
                return
            try:
                setattr(self.value, field.name, value)
            except Exception as e:
                print(u'Error generating value for {}.{}: {}'.format(
                    stone_type.name, field.name, e))
                raise

    def _generate_field_value(self, field, typ, rust_expr_extra = ''):
        field_name = self._rust_generator._field_name(field)
        typ, option = data_type.unwrap_nullable(typ)
        if option:
            if data_type.is_numeric_type(typ) or data_type.is_boolean_type(typ):
                rust_expr = field_name + rust_expr_extra + '.unwrap()'
            else:
                rust_expr = field_name + rust_expr_extra + '.as_ref().unwrap()'
        else:
            rust_expr = field_name + rust_expr_extra

        value = None
        if data_type.is_struct_type(typ):
            inner = TestStruct(self._rust_generator, typ, self._reference_impls)
            if inner.value is None:
                return None
            for inner_rust_expr, inner_rust_value in inner.leaves:
                self.leaves.append((rust_expr + '.' + inner_rust_expr, inner_rust_value))
            value = inner.value
        elif data_type.is_union_type(typ):
            return None # TODO
        elif data_type.is_numeric_type(typ):
            value = typ.max_value or typ.maximum or 1e307
            self.leaves.append((rust_expr, str(value)))
        elif data_type.is_string_type(typ):
            if typ.pattern:
                value = Unregex(typ.pattern, typ.min_length).generate()
            elif typ.min_length:
                value = 'a' * typ.min_length
            else:
                value = 'something'

            rust_value = u'r#"{}"#'.format(value)
            self.leaves.append((rust_expr + '.as_str()', rust_value))
        elif data_type.is_boolean_type(typ):
            value = True
            self.leaves.append((rust_expr, 'true'))
        elif data_type.is_timestamp_type(typ):
            value = datetime.datetime.utcfromtimestamp(2**33 - 1)
            rust_value = u'"{}"'.format(value.strftime(typ.format))
            self.leaves.append((rust_expr + '.as_str()', rust_value))
        elif data_type.is_list_type(typ):
            if option:
                rust_expr_extra += '.as_ref().unwrap()'
            rust_expr_extra += '.get(0).as_ref().unwrap()'
            value = self._generate_field_value(field, typ.data_type, rust_expr_extra)
            if value is None:
                return None
            value = [value]
        else:
            print(u'Error: unhandled field type of {}.{}: {}'.format(
                self._stone_type, field.name, typ))
            return None

        return value

# Generate a minimal string that passes a regex and optionally is of a given minimum length.
class Unregex:
    def __init__(self, regex_string, min_len = None):
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
                pass # start or end anchor; nothing to add
            elif opcode == 'in':
                if argument[0][0] == 'negate':
                    rejects = []
                    for opcode, reject in argument[1:]:
                        if opcode == 'literal':
                            rejects.append(chr(reject))
                        elif opcode == 'range':
                            for i in range(reject[0], reject[1]):
                                rejects.append(chr(i))
                    choices = list(set(string.printable).difference(string.whitespace).difference(rejects))
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
                    or opcode == 'negate': # note: 'negate' is handled in the 'in' opcode
                raise NotImplementedError('regex opcode {} not implemented'.format(opcode))
            else:
                raise NotImplementedError('unknown regex opcode: {}'.format(opcode))
        return result
