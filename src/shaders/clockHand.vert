#version 460 core
layout (location = 0) in vec3 a_position;
layout (location = 1) in vec2 a_texCoordinate;
layout (location = 2) in vec3 a_normal;

layout (std140, binding = 0) uniform Camera {
    mat4 projection;
    mat4 view;
};

uniform mat4 model;

void main() {
    gl_Position = projection * view * model * vec4(a_position, 1.0);
}