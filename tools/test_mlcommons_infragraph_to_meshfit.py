import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

import yaml

MODULE_PATH = Path(__file__).with_name("mlcommons_infragraph_to_meshfit.py")
SPEC = importlib.util.spec_from_file_location("mlcommons_infragraph_to_meshfit", MODULE_PATH)
adapter = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(adapter)


class AdapterTests(unittest.TestCase):
    def test_converts_instances_without_inventing_runtime_evidence(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            src = root / "src"
            out = root / "out"
            src.mkdir()

            graph = {
                "name": "two-node",
                "instances": [
                    {
                        "name": "node-a",
                        "device": "host-type",
                        "count": 1,
                        "description": "mlperf-system-info-single-node-0",
                    }
                ],
            }
            graph_path = root / "infragraph.yaml"
            graph_path.write_text(yaml.safe_dump(graph), encoding="utf-8")

            stem = "mlperf-system-info-single-node-0"
            (src / f"{stem}.json").write_text(
                json.dumps(
                    {
                        "host_processor_model_name": "AMD EPYC 9654",
                        "host_memory_capacity": "256GiB",
                        "accelerator_model_name": "NVIDIA H100 80GB HBM3",
                        "accelerators_per_node": 2,
                        "accelerator_memory_capacity": "80GiB",
                        "inference_backend": "CUDA 12.9",
                        "driver": "Driver 575.57.08",
                        "operating_system": "ubuntu 24.04",
                    }
                ),
                encoding="utf-8",
            )
            (src / f"{stem}.lstopo.xml").write_text(
                """<topology><object type="Machine"><info name="HostName" value="node-a"/>
                <info name="Architecture" value="x86_64"/>
                <info name="OSName" value="Linux"/></object></topology>""",
                encoding="utf-8",
            )

            files = adapter.convert(graph_path, src, out)
            self.assertEqual(len(files), 2)

            discovery = yaml.safe_load((out / "discovery-node-a.yaml").read_text())
            self.assertEqual(discovery["hardware_identity"]["architecture"], "x86_64")
            self.assertEqual(discovery["hardware_identity"]["operating_system"], "linux")
            self.assertIsNone(discovery["hardware_identity"]["ram_mib"])
            self.assertEqual(len(discovery["hardware_identity"]["devices"]), 2)
            self.assertEqual(discovery["hardware_identity"]["devices"][0]["backend"], "cuda")
            self.assertEqual(discovery["node"]["ram_gb"], 256.0)
            self.assertEqual(discovery["node"]["accelerators"][0]["relative_compute"], 0.0)
            self.assertIsNone(discovery["node"]["accelerators"][0]["free_memory_gb"])
            self.assertEqual(discovery["local_fabric"], [])
            self.assertTrue(any("not execution attestation" in w for w in discovery["warnings"]))

            manifest = yaml.safe_load((out / "snapshot-manifest.yaml").read_text())
            self.assertEqual(manifest["discovery_files"], ["discovery-node-a.yaml"])
            self.assertEqual(manifest["probes"], [])

    def test_refuses_missing_architecture(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            src = root / "src"
            out = root / "out"
            src.mkdir()

            graph_path = root / "infragraph.yaml"
            graph_path.write_text(
                yaml.safe_dump(
                    {
                        "instances": [
                            {
                                "name": "node-a",
                                "description": "mlperf-system-info-single-node-0",
                            }
                        ]
                    }
                ),
                encoding="utf-8",
            )
            stem = "mlperf-system-info-single-node-0"
            (src / f"{stem}.json").write_text(
                json.dumps(
                    {
                        "host_processor_model_name": "AMD EPYC",
                        "host_memory_capacity": "256GiB",
                        "accelerator_model_name": "NVIDIA H100",
                        "accelerators_per_node": 1,
                        "accelerator_memory_capacity": "80GiB",
                        "inference_backend": "CUDA",
                        "operating_system": "linux",
                    }
                ),
                encoding="utf-8",
            )
            (src / f"{stem}.lstopo.xml").write_text(
                "<topology><object type=\"Machine\"></object></topology>",
                encoding="utf-8",
            )

            with self.assertRaisesRegex(ValueError, "refusing to invent hardware identity"):
                adapter.convert(graph_path, src, out)


if __name__ == "__main__":
    unittest.main()
