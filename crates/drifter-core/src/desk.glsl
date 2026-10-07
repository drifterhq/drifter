uniform vec4 U;
float rbox(vec2 p,vec2 b,float r){vec2 q=abs(p)-b+r;return min(max(q.x,q.y),0.)+length(max(q,0.))-r;}
vec3 sky_wall(vec2 uv,float asp){
 vec2 p=vec2(uv.x*asp,uv.y);
 vec3 col=mix(S1,S0,smoothstep(0.,1.,uv.y));
 vec2 sp=p*70.;float s=hs(floor(sp));
 float star=step(.985,s)*(1.-smoothstep(0.,.45*(.3+.7*fract(s*977.)),length(fract(sp)-.5)));
 col+=star*(1.-L)*smoothstep(.2,.7,uv.y);
 vec3 ac=vec3(0.);
 for(int i=0;i<3;i++){float fi=float(i);
  float x=p.x*(.8+fi*.35)+fi*7.;
  float band=.62+fi*.08+.14*(fbm(vec2(x*.8,T*.04+fi))-.5)+.05*sin(x*1.7+T*.06);
  float d=uv.y-band;
  float curtain=exp(-d*d*(d<0.?220.:18.));
  float rays=.45+.55*fbm(vec2(x*10.,T*.12+fi*3.));
  vec3 c=mix(A,B,smoothstep(-.02,.22,d));c=mix(c,C,fi*.3);
  ac+=c*curtain*rays*(1.-fi*.22);
 }
 col+=ac*mix(1.15,.4,L);
 return col;
}
vec4 desk(vec2 P){
 vec2 c=P-R*0.5;
 float edge=rbox(c,R*0.5,12.);
 float a=clamp(0.5-edge,0.,1.);
 if(a<=0.)return vec4(0.);
 vec2 uv=P/R;
 vec3 col=sky_wall(vec2(uv.x,uv.y*0.8+0.2),R.x/R.y*0.55);
 float bar=step(R.y-11.,P.y);
 col=mix(col,mix(vec3(0.08,0.09,0.12),vec3(0.96,0.96,0.98),L*0.8+0.1),bar*0.78);
 col=mix(col,vec3(1.),bar*step(abs(P.x-(R.x-26.)),9.)*step(abs(P.y-(R.y-5.5)),1.5)*0.7);
 float show=U.x;
 vec2 dk=vec2(R.x*0.5,10.+(1.-show)*-30.);
 float pill=rbox(P-dk-vec2(0.,1.),vec2(68.,10.),9.);
 float pa=clamp(0.5-pill,0.,1.);
 col=mix(col,col*0.6,exp(-max(pill,0.)*0.25)*0.35*step(0.,pill)*show);
 col=mix(col,mix(vec3(0.12,0.13,0.17),vec3(0.95,0.95,0.97),L*0.8+0.1),pa*0.82);
 for(int i=0;i<5;i++){
  float fi=float(i);
  vec2 ic=dk+vec2((fi-2.)*24.,1.);
  float sq=rbox(P-ic,vec2(7.5),3.);
  vec3 tint=i==0?A:(i==1?B:(i==2?C:(i==3?S0:mix(A,B,0.5))));
  col=mix(col,tint*0.9+0.08,clamp(0.5-sq,0.,1.)*pa);
 }
 col*=1.-0.18*smoothstep(-6.,0.,edge);
 return vec4(col*a,a);
}
void main(){
 vec2 P=vec2(v_coords.x,1.-v_coords.y)*R;
 gl_FragColor=clamp(desk(P),0.,1.)*alpha;
}
