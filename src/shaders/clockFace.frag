#version 460 core
precision mediump float;

out vec4 f_fragColor;

const float radius = 400.0;
const vec2 center = vec2(400, 400);

float distanceFromCenter() {
    float x_component = pow(gl_FragCoord.x - center.x, 2);
    float y_component = pow(gl_FragCoord.y - center.y, 2);

    return sqrt(x_component + y_component);
}

void main() {
    if (distanceFromCenter() <= radius) {
        f_fragColor = vec4(0.2, 0.2, 0.3, 1.0);
    } else {
        f_fragColor = vec4(0.0, 0.0, 0.0, 0.0);
    }
}