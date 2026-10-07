#version 100

//_DEFINES_

#if defined(EXTERNAL)
#extension GL_OES_EGL_image_external : require
#endif

#ifdef GL_FRAGMENT_PRECISION_HIGH
precision highp float;
#else
precision mediump float;
#endif
#if defined(EXTERNAL)
uniform samplerExternalOES tex;
#else
uniform sampler2D tex;
#endif

uniform float alpha;
varying vec2 v_coords;

#if defined(DEBUG_FLAGS)
uniform float tint;
#endif

uniform vec2 W;
uniform float L;
uniform vec3 S0;
uniform vec3 S1;
uniform vec3 A;
uniform vec3 B;
uniform vec3 C;

vec3 lines(vec2 p){
 vec3 base=mix(S1,S0,smoothstep(-0.9,0.9,p.y));
 float s=p.x*0.8+p.y*0.6;
 float band=fract(s*7.);
 vec3 c=mix(A,B,0.5+0.5*sin(s*3.));
 return mix(base,c,smoothstep(0.4,0.55,band)*smoothstep(1.,0.85,band)*0.5);
}

void main(){
 vec2 px=1./W;
 vec4 m=texture2D(tex,v_coords);
 float cov=m.r;
 float hgt=m.g;
 float e=2.;
 float hl=texture2D(tex,v_coords-vec2(px.x*e,0.)).g;
 float hr=texture2D(tex,v_coords+vec2(px.x*e,0.)).g;
 float hu=texture2D(tex,v_coords-vec2(0.,px.y*e)).g;
 float hd=texture2D(tex,v_coords+vec2(0.,px.y*e)).g;
 float k=W.y*0.045/e;
 vec3 nn=normalize(vec3(-(hr-hl)*k,(hd-hu)*k,1.));
 vec2 p=(v_coords*W-W*0.5)/W.y*2.;
 p.y=-p.y;
 vec2 base=p*(1.-0.22*hgt);
 vec2 off=-nn.xy*0.42;
 vec3 col=vec3(lines(base+off*1.08).r,lines(base+off).g,lines(base+off*0.92).b);
 col=col*1.08+0.03;
 vec3 ld=normalize(vec3(-0.5,0.7,0.6));
 float spec=pow(max(dot(reflect(-ld,nn),vec3(0.,0.,1.)),0.),70.);
 float soft=pow(max(dot(reflect(-ld,nn),vec3(0.,0.,1.)),0.),8.);
 float fres=pow(1.-nn.z,2.2);
 col=mix(col,mix(S0,vec3(1.),0.7),fres*0.45);
 col+=vec3(1.)*(spec*1.4+soft*0.12);
 float rim=cov*(1.-smoothstep(0.18,0.5,hgt));
 vec2 gd=normalize(vec2(hr-hl,-(hd-hu))+1e-6);
 col+=vec3(1.)*rim*0.35*(0.3+0.7*max(dot(gd,normalize(vec2(0.6,-0.8))),0.));
 col+=mix(A,C,0.5)*rim*0.3*max(dot(gd,normalize(vec2(-0.6,0.8))),0.);
 vec2 drop=vec2(0.012*W.y*px.x,0.06);
 float sh=texture2D(tex,v_coords-drop).b*0.32;
 float s=sh*(1.-cov);
 gl_FragColor=vec4(clamp(col,0.,1.)*cov,cov+s)*alpha;
}
