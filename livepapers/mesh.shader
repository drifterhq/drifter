vec3 wall(vec2 P){
 vec2 uv=P/R; float asp=R.x/R.y; vec2 p=vec2(uv.x*asp,uv.y);
 float t=T*.035;
 p+=.14*vec2(fbm(p*1.4+t),fbm(p*1.4-t+5.))-.07;
 vec2 q0=vec2(asp*(.2+.15*sin(t*1.3)),.82+.1*cos(t*1.1));
 vec2 q1=vec2(asp*(.8+.12*cos(t*.9)),.75+.15*sin(t*1.4));
 vec2 q2=vec2(asp*(.5+.25*sin(t*.7+1.)),.45+.15*cos(t*.8));
 vec2 q3=vec2(asp*(.15+.1*cos(t*1.2+2.)),.2+.12*sin(t));
 vec2 q4=vec2(asp*(.85+.1*sin(t*1.1+3.)),.15+.1*cos(t*1.3));
 float w0=1./pow(dot(p-q0,p-q0)+.02,1.6), w1=1./pow(dot(p-q1,p-q1)+.02,1.6),
       w2=1./pow(dot(p-q2,p-q2)+.02,1.6), w3=1./pow(dot(p-q3,p-q3)+.02,1.6),
       w4=1./pow(dot(p-q4,p-q4)+.02,1.6);
 vec3 col=(S0*w0+A*w1+B*w2+C*w3+S1*w4)/(w0+w1+w2+w3+w4);
 col*=1.-.25*length(uv-.5);
 return col;
}
