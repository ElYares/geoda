"""Pruebas de `frequent_itemsets` y `association_rules` (fase 6) y de los casos de
`spec/cases/apriori/` por el binding nativo."""

import json
import unittest
from pathlib import Path

import geoda

CASES = sorted((Path(__file__).resolve().parents[3] / "spec" / "cases" / "apriori").glob("*.json"))
EXAMPLE = [[1, 2, 3], [1, 2], [1, 3], [2, 3]]


def _transactions(case):
    """Reagrupa el formato largo de un caso en una lista de transacciones."""
    grouped = {}
    for tx, item in zip(case["input"]["values"], case["input2"]["values"]):
        grouped.setdefault(tx, []).append(item)
    return list(grouped.values())


class FrequentItemsets(unittest.TestCase):
    def test_classic_example(self):
        result = geoda.frequent_itemsets(EXAMPLE, 2)
        self.assertEqual(result["itemsets"], [[1], [2], [3], [1, 2], [1, 3], [2, 3]])
        self.assertEqual(result["support"], [3, 3, 3, 2, 2, 2])
        self.assertEqual(result["transactions"], 4)

    def test_repeated_items_count_once(self):
        self.assertEqual(geoda.frequent_itemsets([[5, 5, 5], [5]], 2)["support"], [2])

    def test_support_above_the_transactions(self):
        self.assertEqual(geoda.frequent_itemsets(EXAMPLE, 5)["itemsets"], [])

    def test_empty(self):
        self.assertEqual(
            geoda.frequent_itemsets([], 1), {"itemsets": [], "support": [], "transactions": 0}
        )

    def test_the_order_is_stable(self):
        first = geoda.frequent_itemsets(EXAMPLE, 2)
        for _ in range(3):
            self.assertEqual(geoda.frequent_itemsets(EXAMPLE, 2), first)

    def test_errors(self):
        with self.assertRaises(ValueError):
            geoda.frequent_itemsets(EXAMPLE, 0)
        with self.assertRaises(ValueError):
            geoda.frequent_itemsets([[1, -2]], 1)


class AssociationRules(unittest.TestCase):
    def test_classic_example(self):
        rules = geoda.association_rules(EXAMPLE, 2, 0.6)
        self.assertEqual(len(rules), 6)
        first = rules[0]
        self.assertEqual((first["antecedent"], first["consequent"]), ([1], [2]))
        self.assertEqual(first["support"], 2)
        self.assertAlmostEqual(first["confidence"], 2 / 3)
        self.assertAlmostEqual(first["lift"], 8 / 9)

    def test_confidence_filter(self):
        self.assertEqual(geoda.association_rules(EXAMPLE, 2, 0.7), [])

    def test_a_perfect_rule(self):
        rules = geoda.association_rules([[1, 2], [1, 2], [3]], 2, 1.0)
        self.assertEqual(
            [(r["antecedent"], r["consequent"]) for r in rules], [([1], [2]), ([2], [1])]
        )
        self.assertTrue(all(r["confidence"] == 1.0 for r in rules))

    def test_errors(self):
        for confidence in (0.0, -0.5, 1.5, float("nan")):
            with self.assertRaises(ValueError):
                geoda.association_rules(EXAMPLE, 2, confidence)
        with self.assertRaises(ValueError):
            geoda.association_rules(EXAMPLE, 0, 0.5)


class SpecCases(unittest.TestCase):
    """Los casos `apriori_*` de la spec dan lo mismo por el binding."""

    def test_cases(self):
        ran = 0
        for path in CASES:
            case = json.loads(path.read_text(encoding="utf-8"))
            op, expect, args = case["op"], case["expect"], case["args"]
            with self.subTest(case=case["id"]):
                ran += 1
                # El formato largo del caso puede tener ids negativos o largos distintos, que
                # `_transactions` ya no ve: esos casos son errores de la capi y de la frontera.
                if case["id"] in ("apriori/error_length", "apriori/error_negative_item"):
                    continue
                data = _transactions(case)
                if "error" in expect:
                    with self.assertRaises(ValueError):
                        if op == "apriori_itemsets":
                            geoda.frequent_itemsets(data, args["min_support"])
                        else:
                            geoda.association_rules(
                                data, args["min_support"], args["min_confidence"]
                            )
                    continue
                if op == "apriori_itemsets":
                    result = geoda.frequent_itemsets(data, args["min_support"])
                    want_set, want_item, want_support = (c["values"] for c in expect["columns"])
                    flat_set = [i for i, s in enumerate(result["itemsets"]) for _ in s]
                    flat_item = [x for s in result["itemsets"] for x in s]
                    self.assertEqual(
                        [flat_set, flat_item, result["support"]],
                        [want_set, want_item, want_support],
                    )
                else:
                    found = geoda.frequent_itemsets(data, args["min_support"])["itemsets"]
                    rules = geoda.association_rules(
                        data, args["min_support"], args["min_confidence"]
                    )
                    got = [
                        [found.index(r["antecedent"]) for r in rules],
                        [found.index(r["consequent"]) for r in rules],
                        [r["support"] for r in rules],
                        [r["confidence"] for r in rules],
                        [r["lift"] for r in rules],
                    ]
                    self.assertEqual(got, [c["values"] for c in expect["columns"]])
        self.assertGreaterEqual(ran, 10)


if __name__ == "__main__":
    unittest.main()
