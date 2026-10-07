vec3 wall(vec2 P){
 vec2 uv=P/R; float asp=R.x/R.y; vec2 p=vec2(uv.x*asp,uv.y)*1.7;
 float t=T*.005;
 vec2 q=vec2(fbm(p+vec2(0.,t)),fbm(p+vec2(5.2,1.3)-t));
 vec2 r=vec2(fbm(p+3.*q+vec2(1.7,9.2)+t*.7),fbm(p+3.*q+vec2(8.3,2.8)-t*.5));
 float f=fbm(p+3.*r);
 vec3 col=mix(S1,S0,uv.y);
 col=mix(col,A,smoothstep(.35,.85,f)*.8);
 col=mix(col,B,smoothstep(.5,1.1,length(q))*.6);
 col=mix(col,C,smoothstep(.55,.9,r.x)*.45);
 col*=.82+.35*f;
 return col;
}
