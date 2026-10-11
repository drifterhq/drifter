#ifdef SQUIRCLE
uniform vec4 U;
#endif
float bulge(float d){float k=clamp(-d/0.24,0.,1.);return sqrt(1.-(1.-k)*(1.-k));}
vec3 lines(vec2 p){
 vec3 base=mix(S1,S0,smoothstep(-0.9,0.9,p.y));
 float s=p.x*0.8+p.y*0.6;
 float band=fract(s*7.);
 vec3 c=mix(A,B,0.5+0.5*sin(s*3.));
 return mix(base,c,smoothstep(0.4,0.55,band)*smoothstep(1.,0.85,band)*0.5);
}
float field(vec2 p){
 float an=atan(p.y,p.x);
 float w=0.035*sin(3.*an+T*0.45)+0.025*sin(5.*an-T*0.33)+0.015*sin(2.*an+T*0.21);
#ifdef SQUIRCLE
 float h=smoothstep(0.,1.,U.x);
 float m=0.035*sin(3.*an+T*1.7)+0.03*sin(5.*an-T*1.3)+0.02*sin(2.*an+T*0.9);
 vec2 q=abs(p);
 return (pow(pow(q.x,5.)+pow(q.y,5.),0.2)-0.66-w*0.08-m*h)*0.95;
#else
 return (length(p)-0.55-w)*0.95;
#endif
}
vec4 mark(vec2 uv,float px){
 float d=field(uv);
 vec2 so=vec2(0.025,-0.075);
 float sd=field(uv-so);
 float shadow=(1.-smoothstep(-0.06,0.16,sd))*0.3;
 float focus=1.-smoothstep(-0.18,0.0,field((uv-so-vec2(0.,0.02))*1.35));
 vec3 caustic=mix(mix(A,C,0.5),vec3(1.),0.4)*focus*0.55;
 if(d>0.){
  float s=shadow*(1.-focus*0.6);
  return vec4(caustic*s*1.6,s);
 }
 float e=0.006;
 float hx=bulge(field(uv+vec2(e,0.)))-bulge(field(uv-vec2(e,0.)));
 float hy=bulge(field(uv+vec2(0.,e)))-bulge(field(uv-vec2(0.,e)));
 float hgt=bulge(d);
 vec3 nn=normalize(vec3(-hx/(2.*e)*0.16,-hy/(2.*e)*0.16,1.));
 vec2 base=uv*(1.-0.22*hgt);
 vec2 off=-nn.xy*0.42;
 vec3 col=vec3(lines(base+off*1.08).r,lines(base+off).g,lines(base+off*0.92).b);
 col=col*1.08+0.03;
 vec3 ld=normalize(vec3(-0.5,0.7,0.6));
 float spec=pow(max(dot(reflect(-ld,nn),vec3(0.,0.,1.)),0.),70.);
 float soft=pow(max(dot(reflect(-ld,nn),vec3(0.,0.,1.)),0.),8.);
 float fres=pow(1.-nn.z,2.2);
 col=mix(col,mix(S0,vec3(1.),0.7),fres*0.45);
 col+=vec3(1.)*(spec*1.4+soft*0.12);
 vec2 gd=normalize(vec2(field(uv+vec2(0.002,0.))-field(uv-vec2(0.002,0.)),field(uv+vec2(0.,0.002))-field(uv-vec2(0.,0.002)))+1e-6);
 float rim=smoothstep(-0.016,-0.003,d)*smoothstep(0.,-0.003,d);
 col+=vec3(1.)*rim*(0.25+0.75*max(dot(gd,normalize(vec2(-0.6,0.8))),0.));
 col+=mix(A,C,0.5)*rim*0.5*max(dot(gd,normalize(vec2(0.6,-0.8))),0.);
 col*=1.-0.18*smoothstep(-0.05,-0.01,d)*max(dot(gd,normalize(vec2(0.6,-0.8))),0.);
 float a=smoothstep(0.,-px,d);
 float s=shadow*(1.-a);
 return vec4(col*a+caustic*s,a+s);
}
void main(){
 vec2 P=vec2(v_coords.x,1.-v_coords.y)*R;
 float px=2./R.y;
 vec4 acc=vec4(0.);
 for(int i=0;i<4;i++){
  vec2 o=vec2(mod(float(i),2.),floor(float(i)/2.))*0.5-0.25;
  acc+=mark(((P+o)-R*0.5)/R.y*2.,px);
 }
 gl_FragColor=clamp(acc*0.25,0.,1.)*alpha;
}
