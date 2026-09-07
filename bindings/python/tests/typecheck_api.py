from typing import assert_type

import nucleation


schematic = nucleation.Schematic.create("typed")
assert_type(schematic, nucleation.Schematic)

dimensions = schematic.tight_dimensions()
assert_type(dimensions, nucleation.Dimensions)

effect = nucleation.AnimationEffect.create(250.0)
effect.add_tween("opacity", start=0.0, end=1.0, easing_name="linear")

space = nucleation.InterpolationSpace.Rgb
assert_type(space, nucleation.InterpolationSpace)


def catch_engine_error(schematic: nucleation.Schematic) -> None:
    try:
        schematic.get_block(99, 99, 99)
    except nucleation.NucleationError as error:
        code: nucleation.NucleationErrorCode = error.code
        if code == nucleation.NucleationErrorCode.NotFound:
            return
        raise
