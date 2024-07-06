#version 410 core
precision mediump float;
precision mediump sampler2D;

in vec2 v_texCoordinate;

uniform sampler2D faceTexture;

out vec4 f_fragColor;

void main() {
    f_fragColor = texture(faceTexture, v_texCoordinate);
}