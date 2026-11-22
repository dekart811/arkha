#!/bin/bash
# scene.sh FILE SKY1 SKY2 SUN M1 M2 M3 SEED: a flat landscape picture.
f=$1 s1=$2 s2=$3 sun=$4 m1=$5 m2=$6 m3=$7 seed=$8
ridge() { # ridge BASE AMP SEED -> polygon points
  awk -v b="$1" -v a="$2" -v s="$3" 'BEGIN{srand(s); printf "0,800 "; y=b; for(x=0;x<=1200;x+=60){y+= (rand()-0.5)*a; if(y<b-a*2)y=b-a*2; if(y>b+a*2)y=b+a*2; printf "%d,%d ",x,y} printf "1200,800"}'
}
sx=$(( 250 + seed * 137 % 700 )); sy=$(( 180 + seed * 53 % 160 ))
magick -size 1200x800 "gradient:$s1-$s2" \
  -fill "$sun" -draw "circle $sx,$sy $((sx+70)),$sy" \
  -fill "$m1" -draw "polygon $(ridge 430 70 "$seed")" \
  -fill "$m2" -draw "polygon $(ridge 540 50 $((seed+1)))" \
  -fill "$m3" -draw "polygon $(ridge 650 30 $((seed+2)))" \
  -blur 0x1 -quality 85 "$f"
