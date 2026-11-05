import lodash from 'lodash';
import React from 'react';
import RemoteButton from 'example_guest_2/Button';

const style = {
	background: '#646cff',
	color: '#fff',
	padding: 12,
};

const Button = () => (
	<>
		<button type="button" style={style}>
			Vite Guest Button - lodash {lodash.VERSION}
		</button>
		<React.Suspense fallback="Loading Guest 2 Button">
			<RemoteButton />
		</React.Suspense>
	</>
);

export default Button;
