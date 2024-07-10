#version 460 core
precision lowp float;

in vec3 v_Color;

out vec4 f_FragColor;

void main() {
    f_FragColor = vec4(v_Color, 1.0);
}