#version 410 core
precision mediump float;

uniform vec3 color;

out vec4 f_fragColor;

void main() {
    f_fragColor = vec4(color, 1.0);
}