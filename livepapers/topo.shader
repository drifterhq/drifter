float E(vec2 p,float t){return fbm(p*.9+vec2(t,t*.6))+.5*fbm(p*2.1-t);}
vec3 wall(vec2 P){
 vec2 uv=P/R; float asp=R.x/R.y; vec2 p=vec2(uv.x*asp,uv.y)*2.;
 float t=T*.006; float px=2./R.y;
 float e=E(p,t), ex=E(p+vec2(px,0.),t), ey=E(p+vec2(0.,px),t);
 float k=e*12.; float g=length(vec2(ex-e,ey-e))*12.+1e-5;
 float f=fract(k); float d=min(f,1.-f)/g;
 float major=mod(floor(k+.5),5.)<.5?1.:0.;
 float lw=mix(.7,1.5,major);
 float line=1.-smoothstep(lw,lw+1.,d);
 vec3 col=mix(mix(S1,S0,uv.y),mix(A,B,smoothstep(.3,1.1,e)),.28);
 col=mix(col,mix(C,vec3(1.),.35),line*mix(.45,.85,major));
 return col;
}
