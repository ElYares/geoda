"""Runner de `spec/` en Python (ver `spec/README.md`).

Recorre `spec/cases/**/*.json` y ejecuta cada caso solo con funciones `geoda_*` de la
C-API, cargadas por `ctypes`. Hace lo mismo que `crates/capi/tests/spec.rs`: si los dos
discrepan en un caso, uno de los dos esta mal. Solo usa la biblioteca estandar.
"""

import json
import unittest

import capi

SPEC_VERSION = 1
CASES_DIR = capi.ROOT / "spec" / "cases"
SCALAR_OPS = ("count", "sum", "min", "max", "mean", "var", "std", "corr")
COLUMNS_OPS = ("histogram", "group_by")
OPS = ("roundtrip", "len", "null_count", "slice", "median", "percentile") + SCALAR_OPS + COLUMNS_OPS


class CaseFailure(Exception):
    """El caso no cumple lo que esperaba."""


def run_case(case):
    """Ejecuta un caso. Cualquier cosa inesperada lanza `CaseFailure`, incluida una
    `op` desconocida: no se salta en silencio."""
    if case.get("spec_version") != SPEC_VERSION:
        raise CaseFailure(f"spec_version {case.get('spec_version')!r} no soportada")
    op, expect = case.get("op"), case.get("expect")
    if op is None or expect is None:
        raise CaseFailure("faltan op o expect")
    if op not in OPS:
        raise CaseFailure(f"op desconocida: {op!r}")

    column, code = _import(case["input"])
    result = None
    outputs = []  # columnas nuevas de `histogram` y `group_by`; se cierran todas
    try:
        if code == capi.OK:
            if op == "slice":
                args = case.get("args", {})
                for key in ("offset", "len"):
                    if not isinstance(args.get(key), int) or args[key] < 0:
                        raise CaseFailure(f"args.{key} falta o no es un entero >= 0")
                result, code = column.slice(args["offset"], args["len"])
            elif op in ("median", "percentile"):
                if op == "median":
                    value, code = column.median()
                else:
                    if "p" not in case.get("args", {}):
                        raise CaseFailure("args.p falta")
                    value, code = column.percentile(capi.parse_float(case["args"]["p"]))
            elif op in SCALAR_OPS:
                value, integer, code = _run_scalar(op, case, column)
            elif op in COLUMNS_OPS:
                outputs, code = _run_columns(op, case, column)
            else:
                result = column
                column = None  # `result` es el dueno; no se libera dos veces
        if code != capi.OK:
            _check_error(expect, code)
        elif "error" in expect:
            raise CaseFailure(f"esperaba el error {expect['error']!r}, pero la operacion tuvo exito")
        elif op in COLUMNS_OPS:
            _check_columns(expect, outputs)
        elif op in SCALAR_OPS and integer:
            _check_int(expect, value)
        elif op in ("median", "percentile") + SCALAR_OPS:
            _check_value(expect, value)
        else:
            _check_counts(expect, result)
            _check_exported(expect, result)
    finally:
        for handle in (result, column, *outputs):
            if handle is not None:
                handle.close()


def _workers(case):
    """`args.workers`: hilos de la operacion; si falta, 1 (0 se acepta para probar el error)."""
    workers = case.get("args", {}).get("workers", 1)
    if not isinstance(workers, int) or isinstance(workers, bool) or workers < 0:
        raise CaseFailure("args.workers no es un entero >= 0")
    return workers


def _run_scalar(op, case, column):
    """Estadistico escalar. `sum`, `min` y `max` llaman a la variante `_i64` si la entrada
    es entera y a la `_f64` si no (ver `spec/README.md`). Devuelve `(valor, es_entero, codigo)`."""
    int_input = case["input"]["dtype"] in ("int32", "int64")
    gives_int = op == "count" or (int_input and op in ("sum", "min", "max"))
    if op in ("sum", "min", "max"):
        value, code = column.scalar(
            f"{op}_{'i64' if int_input else 'f64'}", _workers(case), integer=int_input
        )
    elif op in ("var", "std"):
        ddof = case.get("args", {}).get("ddof")
        if not isinstance(ddof, int) or isinstance(ddof, bool) or ddof < 0:
            raise CaseFailure("args.ddof falta o no es un entero >= 0")
        value, code = column.scalar(op, ddof, _workers(case))
    elif op == "corr":
        if "input2" not in case:
            raise CaseFailure("input2 falta")
        other, code = _import(case["input2"])
        if code != capi.OK:
            return None, False, code
        try:
            value, code = column.scalar("corr", other.handle, _workers(case))
        finally:
            other.close()
    else:
        args = (_workers(case),) if op == "mean" else ()
        value, code = column.scalar(op, *args, integer=gives_int)
    return value, gives_int, code


def _run_columns(op, case, column):
    """`histogram` y `group_by`: devuelven `([columnas], codigo)`."""
    args = case.get("args", {})
    if op == "histogram":
        n_bins = args.get("n_bins")
        if not isinstance(n_bins, int) or isinstance(n_bins, bool) or n_bins < 0:
            raise CaseFailure("args.n_bins falta o no es un entero >= 0")
        value_range = args.get("range")
        if value_range is not None:
            if len(value_range) != 2:
                raise CaseFailure("args.range debe ser [lo, hi]")
            value_range = tuple(capi.parse_float(v) for v in value_range)
        return column.histogram(n_bins, value_range, _workers(case))
    agg = args.get("agg")
    if agg not in capi.AGG:
        raise CaseFailure(f"args.agg desconocido: {agg!r}")
    ddof = 0
    if agg in ("var", "std"):
        ddof = args.get("ddof")
        if not isinstance(ddof, int) or isinstance(ddof, bool) or ddof < 0:
            raise CaseFailure("args.ddof falta o no es un entero >= 0")
    if "input2" not in case:
        raise CaseFailure("input2 falta")
    values, code = _import(case["input2"])
    if code != capi.OK:
        return [], code
    try:
        return column.group_by(values, agg, ddof, _workers(case))
    finally:
        values.close()


def _check_columns(expect, outputs):
    wanted = expect.get("columns")
    if not isinstance(wanted, list):
        raise CaseFailure("expect.columns falta (debe ser una lista)")
    if len(wanted) != len(outputs):
        raise CaseFailure(
            f"expect.columns tiene {len(wanted)} entradas y la operacion devolvio {len(outputs)} columnas"
        )
    for i, (want, handle) in enumerate(zip(wanted, outputs)):
        try:
            _check_exported(want, handle)
        except CaseFailure as e:
            raise CaseFailure(f"columns[{i}]: {e}") from None


def _import(spec_input):
    schema, array = capi.build_arrow(spec_input["dtype"], spec_input["values"])
    column, code = capi.from_arrow(schema, array)
    # El esquema siempre es nuestro; el array solo si la capi no lo consumio.
    capi.release_exported(schema, array)
    return column, code


def _check_error(expect, code):
    expected = expect.get("error")
    if expected is None:
        raise CaseFailure(f"fallo inesperado: {capi.category(code)} ({capi.last_error()})")
    if expected != capi.category(code):
        raise CaseFailure(
            f"esperaba el error {expected}, salio {capi.category(code)} ({capi.last_error()})"
        )


def _check_int(expect, actual):
    want = expect.get("value")
    if isinstance(want, bool) or not isinstance(want, int):
        raise CaseFailure("expect.value debe ser un entero de JSON")
    if want != actual:
        raise CaseFailure(f"value: esperado {want!r}, salio {actual!r}")


def _check_value(expect, actual):
    if "value" not in expect:
        raise CaseFailure("expect.value falta")
    want = capi.parse_float(expect["value"])
    if not capi.same_float(want, actual):
        raise CaseFailure(f"value: esperado {want!r}, salio {actual!r}")


def _check_counts(expect, handle):
    actual = {"len": len(handle), "null_count": handle.null_count}
    for key, value in actual.items():
        if key in expect and expect[key] != value:
            raise CaseFailure(f"{key}: esperado {expect[key]!r}, salio {value!r}")


def _check_exported(expect, handle):
    if "dtype" not in expect and "values" not in expect:
        return
    schema, array, code = handle.export()
    if code != capi.OK:
        raise CaseFailure(f"to_arrow fallo: {capi.category(code)} ({capi.last_error()})")
    try:
        dtype, values = capi.read_arrow(schema, array)
    finally:
        capi.release_exported(schema, array)
    if "dtype" in expect and expect["dtype"] != dtype:
        raise CaseFailure(f"dtype: esperado {expect['dtype']!r}, salio {dtype!r}")
    if "values" in expect:
        _check_values(expect["values"], values, dtype)


def _check_values(expected, actual, dtype):
    if len(expected) != len(actual):
        raise CaseFailure(f"values: esperados {len(expected)}, salieron {len(actual)}")
    for i, (want, got) in enumerate(zip(expected, actual)):
        if want is None or got is None:
            ok = want is None and got is None
        elif dtype == "float64":
            ok = capi.same_float(capi.parse_float(want), got)
        else:
            ok = want == got
        if not ok:
            raise CaseFailure(f"values[{i}]: esperado {want!r}, salio {got!r}")


def inline(op, expect, **overrides):
    """Caso en memoria para probar al propio runner."""
    case = {
        "spec_version": 1,
        "id": "inline",
        "description": "caso en memoria",
        "input": {"dtype": "int32", "values": [1, None, 3]},
        "op": op,
        "args": {"offset": 0, "len": 9},
        "expect": expect,
    }
    case.update(overrides)
    return case


class SpecCases(unittest.TestCase):
    def test_all_cases_pass(self):
        files = sorted(CASES_DIR.rglob("*.json"))
        self.assertGreaterEqual(len(files), 4, f"se esperaban al menos 4 casos, hay {len(files)}")
        for path in files:
            name = path.relative_to(CASES_DIR).as_posix()
            with self.subTest(case=name):
                case = json.loads(path.read_text(encoding="utf-8"))
                self.assertEqual(case["id"], name.removesuffix(".json"), "id distinto de la ruta")
                run_case(case)


class RunnerBehaviour(unittest.TestCase):
    def test_unknown_op_fails(self):
        with self.assertRaisesRegex(CaseFailure, "op desconocida"):
            run_case(inline("explode", {"len": 3}))

    def test_len_op_reports_len(self):
        run_case(inline("len", {"len": 3}))
        with self.assertRaises(CaseFailure):
            run_case(inline("len", {"len": 4}))

    def test_error_passes_only_with_that_category(self):
        run_case(inline("slice", {"error": "OUT_OF_BOUNDS"}))
        with self.assertRaisesRegex(CaseFailure, "OUT_OF_BOUNDS"):
            run_case(inline("slice", {"error": "TYPE_MISMATCH"}))

    def test_error_expected_but_operation_succeeds_fails(self):
        with self.assertRaisesRegex(CaseFailure, "tuvo exito"):
            run_case(inline("len", {"error": "FFI"}))

    def test_unsupported_format_is_reported_by_the_capi(self):
        run_case(
            inline(
                "roundtrip",
                {"error": "UNSUPPORTED"},
                input={"dtype": "decimal128", "values": [1, 2]},
            )
        )

    def test_value_is_compared_bit_by_bit(self):
        def case(value):
            return inline(
                "median",
                {"value": value},
                input={"dtype": "int64", "values": [1, 2, 3, 4]},
            )

        run_case(case(2.5))
        for wrong in (2.0, "NaN"):
            with self.assertRaises(CaseFailure):
                run_case(case(wrong))

    def test_percentile_without_p_fails(self):
        with self.assertRaisesRegex(CaseFailure, "args.p"):
            run_case(inline("percentile", {"value": 1.0}, args={}))

    def test_wrong_spec_version_fails(self):
        with self.assertRaises(CaseFailure):
            run_case(inline("len", {"len": 3}, spec_version=2))

    def test_abi_version(self):
        self.assertEqual(capi.abi_version(), 1)

    def test_import_consumes_array_exactly_once(self):
        before = capi.array_releases()
        schema, array = capi.build_arrow("int64", [1, None, 3])
        column, code = capi.from_arrow(schema, array)
        self.assertEqual(code, capi.OK)
        self.assertEqual(capi.array_releases(), before + 1, "la capi debe llamar a release una vez")
        capi.release_exported(schema, array)  # solo el esquema: el array ya no tiene release
        self.assertEqual(capi.array_releases(), before + 1)
        column.close()


if __name__ == "__main__":
    unittest.main()
