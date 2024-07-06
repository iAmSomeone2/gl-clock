#version 410 core
layout (location = 0) in vec3 a_position;
layout (location = 1) in vec2 a_texCoordinate;
layout (location = 2) in vec3 a_normal;

uniform mat4 transformation;

void main() {
    gl_Position = transformation * vec4(a_position, 1.0);
}