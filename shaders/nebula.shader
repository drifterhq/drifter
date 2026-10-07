vec3 wall(vec2 P){
 vec2 uv=P/R; float asp=R.x/R.y; vec2 p=(vec2(uv.x*asp,uv.y)-vec2(asp*.5,.5))*2.2;
 float t=T*.004;
 vec2 w=vec2(fbm(p*1.2+t),fbm(p*1.2-t+3.1));
 float d=fbm(p*1.6+2.2*w+vec2(t,-t));
 float core=exp(-dot(p,p)*.3);
 vec3 col=mix(S1,S0,uv.y)*mix(.35,1.,L);
 col+=A*smoothstep(.45,.9,d)*core;
 col+=B*smoothstep(.3,.75,d*w.x*1.6)*core*.8;
 col+=C*pow(d,3.)*.7;
 vec2 sp=P/R.y*260.; float s=hs(floor(sp));
 float star=step(.993,s)*(1.-smoothstep(0.,.45*(.3+.7*fract(s*977.)),length(fract(sp)-.5)))*(.6+.4*sin(T*1.2+s*50.));
 col+=star*(1.-L*.85);
 return col;
}
