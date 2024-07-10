#version 460 core
layout (location = 0) in vec3 a_position;

uniform mat4 transformations[60];

out vec3 v_Color;

const vec3 blue = vec3(0.0, 0.0, 1.0);
const vec3 red = vec3(1.0, 0.0, 0.0);
const vec3 green = vec3(0.0, 1.0, 0.0);
const vec3 yellow = vec3(1.0, 1.0, 0.0);

vec3 getColor() {
    if (gl_InstanceID == 0) {
        return yellow;
    } else if (gl_InstanceID % 15 == 0) {
        return green;
    } else if (gl_InstanceID % 5 == 0) {
        return red;
    } else {
        return blue;
    }
}

const float offsetAmt = 1.0 / 60.0;

void main() {
    vec4 position = transformations[gl_InstanceID] * vec4(a_position.xyz, 1.0);
    gl_Position = position;
    v_Color = getColor();

}