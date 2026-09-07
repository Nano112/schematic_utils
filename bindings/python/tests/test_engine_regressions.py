"""Public-wheel regressions for GitHub #39 and #40; also run on bridge-only wheels."""
import base64
import json

import nucleation as n
import pytest


def fixture():
    s = n.Schematic.create('deterministic')
    s.set_block(0, 0, 0, 'minecraft:stone')
    s.set_block(1, 0, 0, 'minecraft:chest')
    s.set_block_entity(1, 0, 0, 'minecraft:chest',
                       '{CustomName:"x",Lock:"y",Items:[{Slot:0b,id:"minecraft:stone",Count:1b,tag:{z:1,a:2}}]}')
    s.add_entity_from_snbt('{id:"minecraft:armor_stand",Pos:[0.0d,1.0d,0.0d],Invisible:1b,NoGravity:1b,Small:1b,Marker:1b,Tags:[z,a]}')
    return s


def export(s):
    return base64.b64decode(s.to_schematic_b64())


@pytest.mark.parametrize('plan', [None, n.TransformPlan.canonical(), n.TransformPlan.registry_safe()])
def test_serialized_artifacts_are_deterministic_and_idempotent(plan):
    data = export(fixture())
    def transform(data):
        s = n.Schematic.from_data(data)
        if plan is not None:
            n.apply_transform(s, plan)
            record = json.loads(s.transformation_history_json())[-1]
            assert record['verification']['idempotence'] == 'passed'
            assert record['verification']['idempotence_format'] == 'sponge_v3'
        return export(s)
    outputs = [transform(data) for _ in range(16)]
    assert all(item == outputs[0] for item in outputs)
    assert transform(outputs[0]) == outputs[0]
    # Independent objects/maps, not just repeat exports of one map seed.
    assert len({export(fixture()) for _ in range(16)}) == 1


def test_engine_errors_are_typed_in_native_and_public_apis(tmp_path):
    assert issubclass(n.NucleationError, Exception)
    assert n.NucleationError is n.core.NucleationError
    s = n.Schematic.create('errors')
    with pytest.raises(n.NucleationError) as caught:
        s.get_block(99, 99, 99)
    assert caught.value.code == n.NucleationErrorCode.NotFound
    assert caught.value.code == n.NucleationError.NotFound
    with pytest.raises(n.NucleationError):
        n.Schematic.from_data(b'not a schematic')
    with pytest.raises(n.NucleationError):
        s.save(tmp_path / 'missing' / 'out.schem')
    with pytest.raises(n.NucleationError):
        n.Schematic.open(tmp_path / 'missing.schem')
    # Programmer errors keep their own type.
    with pytest.raises(TypeError):
        s.get_block('wrong', 0, 0)
    assert s.author() == ''
    assert s.description() == ''


def test_snbt_preserves_nested_content_and_list_order():
    data = export(fixture())
    results = []
    snbt = []
    for _ in range(16):
        s = n.Schematic.from_data(data)
        encoded = s.to_schematic_b64()
        results.append(encoded)
        back = n.Schematic.from_data(base64.b64decode(encoded))
        entity_text = json.loads(back.get_entities_snbt_json())[0]
        assert 'Tags:[z,a]' in entity_text
        snbt.append(entity_text)
    assert len(set(results)) == 1
    assert len(set(snbt)) == 1
