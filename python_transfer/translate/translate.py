values = []
lines = []
with open('spi.txt', 'r') as f:
    for line in f:
        lines.append(line)

for i in range(0, len(lines), 6):
    if lines[i].startswith('for('):
        break
    values.append(lines[i][15:17] + lines[i+1][15:17] + lines[i+2][15:17] + lines[i+3][15:17])

# Write the values to a file vision.txt
with open('vision.txt', 'w') as f:
    for value in values:
        f.write(value + '\n')