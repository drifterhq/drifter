vec3 wall(vec2 P){
 vec2 uv=P/R; float asp=R.x/R.y; vec2 p=vec2(uv.x*asp,uv.y);
 float t=T*.01;
 vec3 col=mix(S1,S0,uv.y);
 for(int i=0;i<5;i++){float fi=float(i);
  float y=.18+fi*.15+.09*sin(p.x*(1.1+fi*.25)+t*(1.+fi*.2)+fi*1.7)+.08*(fbm(vec2(p.x*.7+fi*3.,t*.4))-.5);
  float w=.06+.035*sin(p.x*.8+t*.7+fi*2.);
  float d=abs(uv.y-y)/w;
  float band=1.-smoothstep(0.,1.,d);
  float sheen=pow(max(0.,1.-d),3.)*(.6+.4*L);
  vec3 c=mix(mix(A,B,fract(fi*.37)),C,fract(fi*.61));
  col=mix(col,c*(.65+.7*sheen),band*.7);
 }
 return col;
}
