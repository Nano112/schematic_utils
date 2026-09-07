"""Build configuration regressions; run without an installed extension."""
import importlib.util
import json
import subprocess
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[3]
BINDINGS = ROOT / 'bindings/python'
spec = importlib.util.spec_from_file_location('configure_bindings', BINDINGS / 'configure_bindings.py')
config = importlib.util.module_from_spec(spec)
spec.loader.exec_module(config)


def features(requested):
    metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--no-deps', '--format-version=1'], cwd=ROOT))
    package = next(p for p in metadata['packages'] if p['name'] == 'nucleation')
    return config.expand_features(package['features'], requested)


@pytest.mark.parametrize('requested,present,absent', [
    ('bridge', {'Schematic', 'NucleationError'}, {'Scripting', 'Renderer', 'IoLayout', 'Hdl', 'Design', 'WsProfile'}),
    ('bridge,rendering,mc-tick', {'Renderer', 'ResourcePack', 'TickSimulation'}, {'Scripting', 'IoLayout', 'Hdl', 'Design'}),
    ('bridge,scripting-lua', {'Scripting'}, {'Renderer', 'IoLayout'}),
    ('bridge-full', {'Renderer', 'ResourcePack', 'TickSimulation', 'Scripting', 'IoLayout', 'Hdl', 'Design', 'WsProfile'}, set()),
])
def test_bindings_follow_transitive_cargo_features(requested, present, absent):
    selected = {p.stem.removesuffix('_binding') for p in config.select_bindings(ROOT, BINDINGS, features(requested))}
    assert present <= selected
    assert not absent & selected
    if requested == 'bridge-full':
        assert len(selected) == len(list((BINDINGS / 'src/sub_modules').rglob('*.cpp')))


def test_invalid_features_fail_before_linking():
    with pytest.raises(ValueError, match='must enable bridge'):
        features('rendering')
    with pytest.raises(ValueError, match='Unknown'):
        features('bridge,typo')


def test_item_level_feature_gates_keep_animation_without_gpu_methods():
    disabled, methods = config.disabled_items(ROOT, features('bridge'))
    assert 'VideoConfig' in disabled
    assert 'BuildAnimation' not in disabled
    assert {'render_frames', 'render_gif', 'render_video', 'render_video_with_pack', 'to_animated_glb_b64'} <= methods['BuildAnimation']
    disabled, methods = config.disabled_items(ROOT, features('bridge,rendering'))
    assert 'VideoConfig' not in disabled
    assert not methods.get('BuildAnimation')
