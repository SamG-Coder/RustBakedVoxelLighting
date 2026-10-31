# Generated material prompts

Provider: built-in image_gen tool. Requested resolution: 2048 × 2048. Returned resolution: 1254 × 1254 for all thirteen assets. Original files were copied into this project's public/textures directory. No KCD textures were copied.

## Bark

Use case: photorealistic-natural. Game PBR material base color texture: seamless tileable oak tree bark, flat orthographic scan, full frame bark with fine vertical fissures, soft gray brown weathered wood and subtle moss flecks. Uniform neutral diffuse lighting, no baked shadows or highlights, no perspective, no borders, no text. High resolution 2048x2048.

## Oak foliage

Use case: photorealistic-natural. Asset type: high resolution 2048x2048 game foliage cutout texture. A single dense small oak branch spray with many individually detailed green oak leaves, fine visible veins, varied fresh olive and forest greens, natural irregular silhouette. Branch fills central 90 percent of square with transparent space around and between leaves. Front view, evenly lit diffuse base color, no cast shadow, no backdrop, no text. Designed to map onto foliage cards in a realistic tree canopy.

Transparent background requested and preserved.

## Grass

Use case: photorealistic-natural. Asset type: high resolution 2048x2048 game meadow grass clump cutout texture. One lush tuft of narrow wild grass blades, varied lengths, fine blade detail, gently arching tips, some delicate straw seed stalks, natural sage and olive greens. All blades rooted along bottom 8 percent, entire tuft contained in image with transparent margins. Frontal orthographic view, neutral even diffuse illumination without baked shadow, no ground, no text, no backdrop. Intended for crossed grass cards in a realistic landscape.

Transparent background requested and preserved.

## Dirt

Use case: photorealistic-natural. Asset type: seamless tileable high resolution 2048x2048 PBR game ground base color texture. Dry brown woodland dirt, small pale pebbles, granular earth, tiny root fragments, sparse dead grass fragments. Flat orthographic top down surface scan, physically plausible fine detail, uniform neutral diffuse lighting, no directional shadows, no large recognizable objects, no borders, no text. Seamless repeating edges.

## Normal-map derivation

The browser GPU samples neighbouring luminance values to obtain dx and dy, normalizes (-strength*dx, -strength*dy, 1), and packs it to RGB. Transparent source texels have zero height. Strength is 6 for bark and 3 for the other maps. This derives an approximate normal field; it does not recover true geometric height from a photograph.


## Additional generated materials

The initial four exact prompts above are retained. Subsequent generated assets extend the same neutral-lighting material direction: birch and ash foliage, meadow grass, clover, rock, dry and lush meadow ground, and birch and ash bark. Their installed source names are birch.png, ash.png, meadow.png, clover.png, rock.png, meadow-ground.png, lush-ground.png, bark-birch.png and bark-ash.png. Foliage and grass use transparent cutouts; ground and bark use opaque surface textures. These descriptions are a material inventory, not verbatim generation transcripts. All thirteen sources and their exported approximate normals are 1254 x 1254.
