import AppKit

/// Menu row: label above a slider, sized by Auto Layout from the controls so the knob is never clipped.
func sliderRow(_ label: String, _ s: NSSlider) -> NSView {
    let name = NSTextField(labelWithString: label)
    name.font = NSFont.menuFont(ofSize: 0)
    let v = NSView()
    for c in [name, s] { c.translatesAutoresizingMaskIntoConstraints = false; v.addSubview(c) }
    // The knob overhangs the track and grows while dragged, so it gets room on every side.
    let inset: CGFloat = 16, knobRoom: CGFloat = 8
    NSLayoutConstraint.activate([
        name.leadingAnchor.constraint(equalTo: v.leadingAnchor, constant: inset),
        name.trailingAnchor.constraint(lessThanOrEqualTo: v.trailingAnchor, constant: -inset),
        name.topAnchor.constraint(equalTo: v.topAnchor, constant: 4),
        s.leadingAnchor.constraint(equalTo: v.leadingAnchor, constant: inset + knobRoom),
        s.trailingAnchor.constraint(equalTo: v.trailingAnchor, constant: -(inset + knobRoom)),
        s.topAnchor.constraint(equalTo: name.bottomAnchor, constant: 6),
        s.bottomAnchor.constraint(equalTo: v.bottomAnchor, constant: -8),
        v.widthAnchor.constraint(equalToConstant: 200),
    ])
    v.frame.size = v.fittingSize
    return v
}
