vec3 wall(vec2 P){
 vec2 uv=P/R; float asp=R.x/R.y; vec2 p=vec2(uv.x*asp,uv.y)*1.5;
 float t=T*.006;
 vec2 w=vec2(fbm(p+t),fbm(p-t+2.7));
 float th=fbm(p*1.3+1.5*w+vec2(t,-t))+.5*fbm(p*3.-t*1.5);
 vec3 irid=.5+.5*cos(6.2832*(th*2.2+vec3(0.,.33,.67)));
 vec3 base=mix(S1,S0,uv.y);
 vec3 tint=irid*mix(A,vec3(1.),.55)*mix(.45,1.,L);
 vec3 col=mix(base,base*.35+tint*.8,.6);
 col+=pow(max(0.,th-.85),2.)*4.*mix(C,vec3(1.),.5)*(.3+.7*L);
 return col;
}
