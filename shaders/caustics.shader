vec2 vor(vec2 p,float t){vec2 g=floor(p),f=fract(p);float d1=8.,d2=8.;
 for(int j=-1;j<=1;j++)for(int i=-1;i<=1;i++){vec2 o=vec2(float(i),float(j));
  float r=h(g+o);vec2 c=o+.5+.4*vec2(sin(t+6.28*r),cos(t*1.3+6.28*h(g+o+7.)))-f;
  float d=dot(c,c);if(d<d1){d2=d1;d1=d;}else if(d<d2){d2=d;}}
 return vec2(sqrt(d1),sqrt(d2));}
vec3 wall(vec2 P){
 vec2 uv=P/R; float asp=R.x/R.y; vec2 p=vec2(uv.x*asp,uv.y)*3.;
 float t=T*.05;
 p+=.3*vec2(fbm(p*.5+T*.003),fbm(p*.5-T*.003+4.));
 vec2 m=vec2(M.x/R.y,M.y/R.y)*3.;vec2 dm=p-m;p+=M.z*.25*dm*exp(-dot(dm,dm)*1.5);
 vec2 a=vor(p,t), b=vor(p*1.6+3.,t*1.2);
 float c=pow(1.-smoothstep(0.,.3,a.y-a.x),4.)*.7+pow(1.-smoothstep(0.,.3,b.y-b.x),4.)*.45;
 vec3 col=mix(mix(S1,A,.35),mix(S0,B,.25),uv.y)*mix(.5,1.,L);
 col+=mix(C,vec3(1.),.6)*(.35+.75*L)*c;
 col*=1.-.3*length(uv-.5);
 return col;
}
