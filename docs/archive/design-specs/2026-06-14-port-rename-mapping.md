# Output-pin rename — `out` → semantic names

Clean break (no migration). Every output port currently named `out` gets a
semantic name. Sample `edge()` calls are rewritten in lockstep.

Locked conventions:
- Generic number-spread outputs → `values`
- Constants/controls → by type (`number`, `text`, `bool`)
- Vec2 op results → `vector` (normalize/rotate/scale keep `normalized`/`rotated`/`scaled`)
- Boolean logic → `result` (select → `selected`)

## Map (type → new output name)

### math
abs→absolute · acos→arccosine · add→sum · asin→arcsine · atan→arctangent ·
atan2→angle · ceil→ceiled · clamp→clamped · cos→cosine · cumulative→sums ·
delta→differences · dist→distance · div→quotient · exp→exponential ·
floor→floored · fract→fractional · histogram→counts · lerp→mixed ·
log→logarithm · mag→magnitude · map→mapped · max→max · max-spread→max ·
mean→mean · median→median · min→min · min-spread→min · mod→remainder ·
neg→negated · norm→normalized · pow→power · round→rounded · sin→sine ·
sq→squared · sqrt→root · stddev→stddev · sub→difference · sum→sum · tan→tangent
noise/perlin→noise

### random
gaussian→sample · uniform→sample · wanderer2d→position

### logic  (all → result; select → selected)
and · or · not · eq · neq · gt · gte · lt · lte  → result
select → selected

### convert
to-bool→bool · to-degrees→degrees · to-int→integer · to-number→number ·
to-radians→radians · to-string→text

### spread-ops  (generic → values)
append→values · at→value · at-group→values · cons→values · count→count ·
fibonacci→positions · geometric→values · grid→positions · index-of→index ·
inject→values · interleave→values · linear→values · noise→samples ·
pick→values · permute→values · range→indices · repeat→values ·
resample→values · reverse→values · rotate→values · set-slice→values ·
shuffle→values · sift→values · sine→samples · skip→values · slice→values ·
sort→values · sort-by-key→values · spiral→positions · stack→values ·
take→values · unique→values
(spread/ring already done: positions + angles)

### draw  (shapes → shape; rasters/composites → layer)
arc·ellipse·line·path·point·polygon·quad·rect·triangle → shape
image·image-region·image-tile·text·walker-cluster → layer

### transform  (operation past-tense)
apply-matrix→transformed · blend→blended · cache→rasterized · group→grouped ·
rotate→rotated · scale→scaled · shear-x→sheared · shear-y→sheared ·
translate→translated

### vec2  (uniform vector; normalize/rotate/scale differentiated)
add·from-angle·lerp·limit·make·mul·set-mag·sub → vector
normalize→normalized · rotate→rotated · scale→scaled

### color  (all → color)
color·gradient·hex·hsl·lerp·rgb·set-alpha → color

### value / io / particles / data / string
value/number→number · value/string→text · io/slider→number · io/toggle→bool ·
particles/render-as→layer · particles/trail→layer · data/csv-inline→values ·
string/split→parts · string/concat→text · string/word-indices→indices ·
string/length→length

Already semantic (no change): image/* (`image`), sound/* (`audio`), pixel/*,
color-picker, file, xy-pad, fft, and other multi-output nodes.
