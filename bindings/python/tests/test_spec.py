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
GRAPH_OPS = ("graph_bfs", "graph_components", "graph_topological_order", "graph_dijkstra", "graph_mst")
TEXT_OPS = ("edit_distance", "lis", "huffman_encode", "huffman_decode", "dedupe", "dendrogram_cut", "apriori_itemsets", "apriori_rules")
MATRIX_OPS = ("kmeans", "hierarchical", "tree_fit_predict", "matmul", "lu_solve", "det", "inverse", "qr", "svd", "lstsq", "regression_fit_predict", "pca_fit_transform")
OPS = ("roundtrip", "len", "null_count", "slice", "median", "percentile") + SCALAR_OPS + COLUMNS_OPS + TEXT_OPS + MATRIX_OPS


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
    if op not in OPS + GRAPH_OPS:
        raise CaseFailure(f"op desconocida: {op!r}")
    if "graph" in case:
        return run_graph_case(case, op, expect)
    if "inputs" in case:
        return run_matrix_case(case, op, expect)

    column, code = _import(case["input"])
    result = None
    group_count = None  # `expect.count` de `dedupe`
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
            elif op == "dedupe":
                max_distance = case.get("args", {}).get("max_distance")
                if not isinstance(max_distance, int) or isinstance(max_distance, bool) or max_distance < 0:
                    raise CaseFailure("args.max_distance falta o no es un entero >= 0")
                outputs, code, group_count = column.dedupe(max_distance)
            elif op in TEXT_OPS:
                outputs, code = _run_text(op, case, column)
            else:
                result = column
                column = None  # `result` es el dueno; no se libera dos veces
        if code != capi.OK:
            _check_error(expect, code)
        elif "error" in expect:
            raise CaseFailure(f"esperaba el error {expect['error']!r}, pero la operacion tuvo exito")
        elif op in COLUMNS_OPS + TEXT_OPS:
            _check_columns(expect, outputs)
            if group_count is not None and expect.get("count") != group_count:
                raise CaseFailure(f"count: esperado {expect.get('count')!r}, salio {group_count!r}")
        elif op in SCALAR_OPS and integer:
            _check_int(expect, value)
        elif op in ("median", "percentile") + SCALAR_OPS:
            _check_value(expect, value)
        else:
            _check_counts(expect, result)
            _check_exported(expect, result, _tolerance(expect))
    finally:
        for handle in (result, column, *outputs):
            if handle is not None:
                handle.close()


def run_graph_case(case, op, expect):
    """Consulta de grafo (`graph_*`): las aristas viajan por la capi como columnas `Int64` y los
    pesos como una `Float64`; `expect` compara columnas y los escalares `count`, `total` y
    `components`."""
    if op not in GRAPH_OPS:
        raise CaseFailure(f"op de grafo desconocida: {op!r}")
    spec = case["graph"]
    edges = spec["edges"]
    owned = []  # columnas y grafo de entrada; se cierran todos
    outputs = []
    try:
        columns = [
            _import({"dtype": "int64", "values": [edge[i] for edge in edges]}) for i in (0, 1)
        ]
        if spec.get("weights") is not None:
            columns.append(_import({"dtype": "float64", "values": spec["weights"]}))
        else:
            columns.append((None, capi.OK))
        owned += [c for c, _ in columns if c is not None]
        code = next((c for _, c in columns if c != capi.OK), capi.OK)
        extras = {}
        if code == capi.OK:
            graph, code = capi.graph_new(
                spec["n"], columns[0][0], columns[1][0], columns[2][0], spec["directed"]
            )
            if graph is not None:
                owned.append(graph)
                args = case.get("args", {})
                if op in ("graph_bfs", "graph_dijkstra") and not isinstance(args.get("source"), int):
                    raise CaseFailure("args.source falta o no es un entero >= 0")
                if op == "graph_bfs":
                    outputs, code = graph.bfs(args["source"])
                elif op == "graph_dijkstra":
                    outputs, code = graph.dijkstra(args["source"])
                elif op == "graph_topological_order":
                    outputs, code = graph.topological_order()
                elif op == "graph_components":
                    outputs, code, extras["count"] = graph.components()
                else:
                    outputs, code, extras["total"], extras["components"] = graph.mst()
        if code != capi.OK:
            _check_error(expect, code)
            return
        if "error" in expect:
            raise CaseFailure(f"esperaba el error {expect['error']!r}, pero la operacion tuvo exito")
        _check_columns(expect, outputs)
        for key, actual in extras.items():
            if key not in expect:
                raise CaseFailure(f"expect.{key} falta")
            if isinstance(actual, float):
                if not _close(capi.parse_float(expect[key]), actual, _tolerance(expect)):
                    raise CaseFailure(f"{key}: esperado {expect[key]!r}, salio {actual!r}")
            elif expect[key] != actual:
                raise CaseFailure(f"{key}: esperado {expect[key]!r}, salio {actual!r}")
    finally:
        for handle in (*outputs, *owned):
            handle.close()


def run_matrix_case(case, op, expect):
    """Operaciones sobre una matriz (`kmeans`, `hierarchical`): `inputs` es la lista de columnas,
    que viajan a la capi como un arreglo de handles. `expect` compara columnas y, en `kmeans`,
    `inertia` e `iterations`."""
    if op not in MATRIX_OPS:
        raise CaseFailure(f"op de matriz desconocida: {op!r}")
    owned, outputs, extras = [], [], {}
    try:
        code = capi.OK
        for spec in case["inputs"]:
            column, code = _import(spec)
            if column is not None:
                owned.append(column)
            if code != capi.OK:
                break
        if code == capi.OK:
            args = case.get("args", {})
            if op == "kmeans":
                for key in ("k", "seed"):
                    if not isinstance(args.get(key), int) or args[key] < 0:
                        raise CaseFailure(f"args.{key} falta o no es un entero >= 0")
                outputs, code, inertia, iterations = capi.kmeans(
                    owned,
                    args["k"],
                    args.get("max_iter", 100),
                    capi.parse_float(args.get("tol", 1e-4)),
                    args["seed"],
                )
                extras = {"inertia": inertia, "iterations": iterations}
            elif op in ("matmul", "lu_solve", "det", "inverse", "qr", "svd", "lstsq"):
                outputs, code, extras = _run_linalg(op, case, owned, owned_extra := [])
                owned.extend(owned_extra)
            elif op in ("regression_fit_predict", "pca_fit_transform"):
                outputs, code, extras = _run_model(op, case, owned, owned_extra := [])
                owned.extend(owned_extra)
            elif op == "tree_fit_predict":
                outputs, code, extras = _run_tree(case, owned, owned_extra := [])
                owned.extend(owned_extra)
            else:
                outputs, code = capi.hierarchical(owned)
        if code != capi.OK:
            _check_error(expect, code)
            return
        if "error" in expect:
            raise CaseFailure(f"esperaba el error {expect['error']!r}, pero la operacion tuvo exito")
        if op == "det":
            _check_value(expect, extras["value"])
            return
        _check_columns(expect, outputs)
        for key, actual in extras.items():
            if key not in expect:
                continue
            if isinstance(actual, float):
                if not _close(capi.parse_float(expect[key]), actual, _tolerance(expect)):
                    raise CaseFailure(f"{key}: esperado {expect[key]!r}, salio {actual!r}")
            elif expect[key] != actual:
                raise CaseFailure(f"{key}: esperado {expect[key]!r}, salio {actual!r}")
    finally:
        for handle in (*outputs, *owned):
            handle.close()


def _run_model(op, case, train, owned_extra):
    """Regresion o PCA: ajusta con `train`, lee los parametros y predice (o proyecta) las filas de
    `predict_inputs` (por defecto, `train`). Devuelve `([columnas], codigo, extras)`; el modelo se
    libera aqui. Regresion: `[coeficientes, predicciones]` y los extras `intercept` y `r_squared`.
    PCA: `[componentes, varianza, razon, media, proyeccion]`."""
    args = case.get("args", {})
    if op == "regression_fit_predict":
        y, code = _import(case["y"])
        if y is not None:
            owned_extra.append(y)
        if code != capi.OK:
            return [], code, {}
        fit_intercept = args.get("fit_intercept", True)
        if not isinstance(fit_intercept, bool):
            raise CaseFailure("args.fit_intercept debe ser un booleano")
        model, code = capi.regression_fit(train, y, fit_intercept)
    else:
        n_components = args.get("n_components")
        if not isinstance(n_components, int) or isinstance(n_components, bool) or n_components < 0:
            raise CaseFailure("args.n_components falta o no es un entero >= 0")
        model, code = capi.pca_fit(train, n_components)
    if code != capi.OK:
        return [], code, {}
    with model:
        extras = {}
        if op == "regression_fit_predict":
            params, code, intercept, r_squared = model.regression_params()
            extras = {"intercept": intercept, "r_squared": r_squared}
        else:
            params, code = model.pca_params()
        if code != capi.OK:
            return [], code, {}
        predict = train
        if "predict_inputs" in case:
            predict = []
            for spec in case["predict_inputs"]:
                column, code = _import(spec)
                if column is not None:
                    owned_extra.append(column)
                if code != capi.OK:
                    return params, code, {}
                predict.append(column)
        output, code = model.predict(predict)
        if code != capi.OK:
            for handle in params:
                handle.close()
            return [], code, {}
        columns = [*params, *output] if op == "pca_fit_transform" else [params[0], *output]
        if op == "regression_fit_predict":
            for handle in params[1:]:
                handle.close()
        return columns, code, extras


def _run_linalg(op, case, a, owned_extra):
    """Algebra lineal basica sobre `a` (la matriz `A` como lista de columnas): `matmul` lee `inputs2`
    (la matriz `B`) y `lu_solve` la columna `input` (el lado derecho). Devuelve
    `([columna plana], codigo, extras)`; `det` devuelve `([], codigo, {"value": determinante})`."""
    if op == "det":
        value, code = capi.det(a)
        return [], code, {"value": value}
    if op == "inverse":
        outputs, code = capi.inverse(a)
        return outputs, code, {}
    if op in ("qr", "svd"):
        outputs, code = getattr(capi, op)(a)
        return outputs, code, {}
    if op == "lstsq":
        b, code = _import(case["input"])
        if b is not None:
            owned_extra.append(b)
        if code != capi.OK:
            return [], code, {}
        outputs, code, residual = capi.lstsq(a, b)
        return outputs, code, {"residual_norm": residual}
    if op == "lu_solve":
        b, code = _import(case["input"])
        if b is not None:
            owned_extra.append(b)
        if code != capi.OK:
            return [], code, {}
        outputs, code = capi.lu_solve(a, b)
        return outputs, code, {}
    other = []
    for spec in case["inputs2"]:
        column, code = _import(spec)
        if column is not None:
            owned_extra.append(column)
        if code != capi.OK:
            return [], code, {}
        other.append(column)
    outputs, code = capi.matmul(a, other)
    return outputs, code, {}


def _run_tree(case, train, owned_extra):
    """Entrena con `train` y `case["labels"]` y predice `predict_inputs` (por defecto, `train`).
    Devuelve `([predicciones], codigo, {"nodes": n})`. El arbol se libera aqui."""
    args = case.get("args", {})
    labels, code = _import(case["labels"])
    if code != capi.OK:
        return [], code, {}
    owned_extra.append(labels)
    tree, code = capi.tree_fit(
        train, labels, args.get("max_depth", 8), args.get("min_samples_leaf", 1)
    )
    if code != capi.OK:
        return [], code, {}
    with tree:
        predict = train
        if "predict_inputs" in case:
            predict = []
            for spec in case["predict_inputs"]:
                column, code = _import(spec)
                if column is not None:
                    owned_extra.append(column)
                if code != capi.OK:
                    return [], code, {}
                predict.append(column)
        outputs, code = tree.predict(predict)
        return outputs, code, {"nodes": tree.node_count()}


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


def _run_text(op, case, column):
    """Operaciones de texto: devuelven `([columnas], codigo)`. `edit_distance` y
    `huffman_decode` leen `input2` como segunda columna."""
    if op == "lis":
        return column.text_op("column_lis")
    if op == "huffman_encode":
        return column.huffman_encode()
    if "input2" not in case:
        raise CaseFailure("input2 falta")
    other, code = _import(case["input2"])
    if code != capi.OK:
        return [], code
    try:
        if op in ("apriori_itemsets", "apriori_rules"):
            args = case.get("args", {})
            min_support = args.get("min_support")
            if not isinstance(min_support, int) or isinstance(min_support, bool) or min_support < 0:
                raise CaseFailure("args.min_support falta o no es un entero >= 0")
            if op == "apriori_itemsets":
                return column.apriori_itemsets(other, min_support)
            if "min_confidence" not in args:
                raise CaseFailure("args.min_confidence falta")
            return column.apriori_rules(other, min_support, capi.parse_float(args["min_confidence"]))
        if op == "dendrogram_cut":
            args = case.get("args", {})
            for key in ("n", "k"):
                if not isinstance(args.get(key), int) or args[key] < 0:
                    raise CaseFailure(f"args.{key} falta o no es un entero >= 0")
            return column.dendrogram_cut(other, args["n"], args["k"])
        name = "column_edit_distance" if op == "edit_distance" else "huffman_decode"
        return column.text_op(name, other)
    finally:
        other.close()


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
            _check_exported(want, handle, _tolerance(expect))
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
    if not _close(want, actual, _tolerance(expect)):
        raise CaseFailure(f"value: esperado {want!r}, salio {actual!r}")


def _check_counts(expect, handle):
    actual = {"len": len(handle), "null_count": handle.null_count}
    for key, value in actual.items():
        if key in expect and expect[key] != value:
            raise CaseFailure(f"{key}: esperado {expect[key]!r}, salio {value!r}")


def _check_exported(expect, handle, tolerance=None):
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
        _check_values(expect["values"], values, dtype, tolerance)


def _tolerance(expect):
    """`expect.tolerance`: error absoluto maximo de los `float64`; sin ella, comparacion exacta."""
    tolerance = expect.get("tolerance")
    if tolerance is None:
        return None
    if isinstance(tolerance, bool) or not isinstance(tolerance, (int, float)) or not tolerance >= 0 \
            or tolerance == float("inf"):
        raise CaseFailure("expect.tolerance debe ser un numero finito >= 0")
    return float(tolerance)


def _close(want, got, tolerance):
    """Igualdad exacta (bit a bit) o, con tolerancia, a distancia absoluta menor o igual que ella.
    Un NaN solo iguala a un NaN y un infinito solo a si mismo."""
    finite = want - want == 0 and got - got == 0  # False para NaN e infinitos
    if tolerance is not None and finite:
        return abs(want - got) <= tolerance
    return capi.same_float(want, got)


def _check_values(expected, actual, dtype, tolerance=None):
    if len(expected) != len(actual):
        raise CaseFailure(f"values: esperados {len(expected)}, salieron {len(actual)}")
    for i, (want, got) in enumerate(zip(expected, actual)):
        if want is None or got is None:
            ok = want is None and got is None
        elif dtype == "float64":
            ok = _close(capi.parse_float(want), got, tolerance)
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

    def test_tolerance_accepts_small_errors_and_rejects_big_ones(self):
        def case(value, tolerance=None):
            expect = {"value": value}
            if tolerance is not None:
                expect["tolerance"] = tolerance
            return inline("median", expect, input={"dtype": "int64", "values": [1, 2, 3, 4]})

        run_case(case(2.5))
        with self.assertRaises(CaseFailure):
            run_case(case(2.5000001))
        run_case(case(2.5000001, 1e-3))
        with self.assertRaises(CaseFailure):
            run_case(case(2.5000001, 1e-9))
        with self.assertRaises(CaseFailure):
            run_case(case(3.0, 0.1))
        with self.assertRaises(CaseFailure):
            run_case(case(2.5, -1.0))

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
