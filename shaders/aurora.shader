vec3 wall(vec2 P){
 vec2 uv=P/R; float asp=R.x/R.y; vec2 p=vec2(uv.x*asp,uv.y);
 vec3 col=mix(S1,S0,smoothstep(0.,1.,uv.y));
 vec2 sp=p*180.; float s=hs(floor(sp));
 float star=step(.995,s)*(1.-smoothstep(0.,.45*(.3+.7*fract(s*977.)),length(fract(sp)-.5)))*(.6+.4*sin(T*(.8+1.4*fract(s*3137.))+fract(s*7919.)*6.2832));
 col+=star*(1.-L)*smoothstep(.15,.6,uv.y);
 vec3 ac=vec3(0.);
 for(int i=0;i<3;i++){float fi=float(i);
  float x=p.x*(.8+fi*.35)+fi*7.;
  float band=.6+fi*.08+.14*(fbm(vec2(x*.8,T*.04+fi))-.5)+.05*sin(x*1.7+T*.06);
  float d=uv.y-band;
  float curtain=exp(-d*d*(d<0.?260.:22.));
  float rays=.45+.55*fbm(vec2(x*10.,T*.12+fi*3.));
  vec3 c=mix(A,B,smoothstep(-.02,.22,d)); c=mix(c,C,fi*.3);
  ac+=c*curtain*rays*(1.-fi*.22);
 }
 col+=ac*mix(1.15,.2,L);
 vec2 dm=(P-M.xy)/R.y;col+=mix(A,C,.5)*M.z*.35*exp(-dot(dm,dm)*30.)*mix(1.,.3,L);
 col+=S1*.18*exp(-uv.y*5.);
 return col;
}
